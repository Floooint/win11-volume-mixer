import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { type AppDetails, commands, events } from "@/bindings";
import { AppAvatar } from "@/components/AppAvatar";
import { useApplyAccent, useSystemAccent } from "@/lib/accent";
import { cn } from "@/lib/utils";

function Field({ label, children, mono }: { label: string; children: ReactNode; mono?: boolean }) {
  return (
    <div className="grid grid-cols-[4.5rem_1fr] gap-2">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className={cn("min-w-0 break-all text-foreground", mono && "font-mono text-[11px]")}>
        {children}
      </dd>
    </div>
  );
}

/**
 * 应用详情浮窗（`details` 窗口）的内容。首次加载时读取要显示的应用，之后通过
 * `details://show` 事件更新；每次渲染完成后报告内容高度，后端据此定位并显示窗口。
 */
export function DetailsView() {
  const [details, setDetails] = useState<AppDetails | null>(null);
  const [accent, setAccent] = useState<string | null>(null);
  const system = useSystemAccent();
  useApplyAccent(accent, system);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // 窗口尺寸即内容尺寸，不需要滚动条（尺寸调整的瞬间也不闪出滚动条）。
    document.documentElement.style.overflow = "hidden";
    const unlisten = events.detailsShow.listen((e) => setDetails(e.payload));
    void commands.getAppDetails().then((current) => current && setDetails(current));
    void commands.getSettings().then((settings) => setAccent(settings.accent));
    return () => void unlisten.then((fn) => fn());
  }, []);

  // 每次内容变化（包括再次显示同一个应用）都要报告高度，后端收到后才显示窗口。
  useLayoutEffect(() => {
    const content = contentRef.current;
    if (!details || !content) return;
    const frame = requestAnimationFrame(() =>
      void commands.detailsReady(Math.ceil(content.getBoundingClientRect().height)),
    );
    return () => cancelAnimationFrame(frame);
  }, [details]);

  if (!details) return null;
  const { app, alias } = details;
  const percent = Math.round(app.volume.volume * 100);
  return (
    <div ref={contentRef} className="flex flex-col gap-3 p-3 text-xs">
      <div className="flex items-center gap-3">
        <AppAvatar key={app.icon ?? ""} app={app} />
        <div className="min-w-0">
          <p className="break-all text-sm font-semibold">{alias ?? app.name}</p>
          {alias && <p className="break-all text-muted-foreground">原名：{app.name}</p>}
        </div>
      </div>
      <dl className="flex flex-col gap-1.5">
        {app.processName && <Field label="进程名">{app.processName}</Field>}
        {app.exePath && <Field label="路径">{app.exePath}</Field>}
        <Field label="标识" mono>
          {app.appId}
        </Field>
        <Field label="状态">
          {[
            app.active ? "正在播放" : "未在播放",
            app.volume.muted ? "已静音" : `音量 ${percent}%`,
            app.sessionCount > 1 ? `${app.sessionCount} 个会话` : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </Field>
      </dl>
    </div>
  );
}
