import { getName, getVersion } from "@tauri-apps/api/app";
import { ChevronDown, ExternalLink, ImageOff } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import appIcon from "../../src-tauri/icons/128x128.png";
import { ABOUT, CREDITS, REFERENCES } from "@/about";
import { commands } from "@/bindings";
import { announceHeightAnimation } from "@/hooks/use-fit-window-height";
import { cn } from "@/lib/utils";

/** 与分组展开 / 折叠相同（见 MainPage）。 */
const EXPAND_TRANSITION = { duration: 0.18, ease: [0.33, 1, 0.68, 1] } as const;

/** 赞助二维码：src/assets/sponsor/ 下的 alipay.* 与 wechat.*，没有时为 undefined。 */
const QR_IMAGES = import.meta.glob<string>("@/assets/sponsor/*.{png,jpg,jpeg,webp}", {
  eager: true,
  import: "default",
});
function findQr(name: string) {
  return Object.entries(QR_IMAGES).find(([path]) =>
    path.split("/").pop()?.startsWith(`${name}.`),
  )?.[1];
}

const PAYMENTS = [
  { key: "alipay", label: "支付宝", image: findQr("alipay") },
  { key: "wechat", label: "微信", image: findQr("wechat") },
] as const;

/** 链接：用默认浏览器打开。留空时正式版不显示，开发模式显示“未填写”。 */
function LinkRow({ title, url, label }: { title: string; url: string; label?: string }) {
  if (!url && !import.meta.env.DEV) return null;
  return (
    <div className="flex min-h-7 items-center justify-between gap-3">
      <span className="shrink-0 text-muted-foreground">{title}</span>
      {url ? (
        <button
          type="button"
          onClick={() => void commands.openUrl(url)}
          className="inline-flex min-w-0 items-center gap-1 rounded-md px-1.5 py-0.5 text-primary hover:bg-accent"
        >
          <span className="truncate">{label ?? url.replace(/^https?:\/\//, "")}</span>
          <ExternalLink size={12} className="shrink-0" />
        </button>
      ) : (
        <span className="text-xs text-muted-foreground/60">未填写（src/about.ts）</span>
      )}
    </div>
  );
}

function InfoRow({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex min-h-7 items-center justify-between gap-3">
      <span className="shrink-0 text-muted-foreground">{title}</span>
      <span className="min-w-0 truncate">{children}</span>
    </div>
  );
}

function SubHeading({ children }: { children: ReactNode }) {
  return <h3 className="mt-1 text-xs font-medium text-muted-foreground">{children}</h3>;
}

/** 赞助：二维码在上，下面的按钮切换支付宝 / 微信。 */
function Sponsor() {
  const [current, setCurrent] = useState<(typeof PAYMENTS)[number]["key"]>("alipay");
  const payment = PAYMENTS.find((p) => p.key === current) ?? PAYMENTS[0];
  return (
    <div className="flex flex-col items-center gap-2">
      <p className="self-stretch text-xs whitespace-pre-line text-muted-foreground">
        {ABOUT.sponsor.text}
      </p>
      {/* 随窗口宽度放大，最大 256 px。 */}
      <div className="flex aspect-square w-full max-w-64 items-center justify-center overflow-hidden rounded-lg border border-border bg-white">
        {payment.image ? (
          <img
            src={payment.image}
            alt={`${payment.label}收款码`}
            className="size-full object-contain"
            draggable={false}
          />
        ) : (
          <span className="flex flex-col items-center gap-1 text-xs text-neutral-400">
            <ImageOff size={20} />
            {payment.label}收款码
          </span>
        )}
      </div>
      <div className="flex gap-2">
        {PAYMENTS.map((p) => (
          <button
            key={p.key}
            type="button"
            aria-pressed={current === p.key}
            onClick={() => setCurrent(p.key)}
            className={cn(
              "h-7 min-w-20 rounded-md border px-3 text-xs transition-colors",
              "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
              current === p.key
                ? "border-primary bg-primary text-primary-foreground"
                : "border-input bg-background hover:bg-accent/60",
            )}
          >
            {p.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function AboutContent() {
  const [name, setName] = useState("");
  const [version, setVersion] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    getName().then(setName, () => {});
    getVersion().then(setVersion, () => {});
  }, []);

  const revealSettings = async () => {
    const result = await commands.revealSettingsFile();
    setError(result.status === "error" ? result.error.message : null);
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-3">
        <img src={appIcon} alt="" className="size-12 shrink-0" draggable={false} />
        <div className="min-w-0">
          <p className="truncate font-semibold">{name}</p>
          <p className="text-xs text-muted-foreground">版本 {version}</p>
          <p className="text-xs text-muted-foreground">{ABOUT.tagline}</p>
        </div>
      </div>

      <div className="flex flex-col">
        <InfoRow title="作者">{ABOUT.author.name}</InfoRow>
        <LinkRow title="B 站主页" url={ABOUT.author.bilibili} />
        <LinkRow title="项目主页" url={ABOUT.homepage} />
        <LinkRow title="反馈问题" url={ABOUT.issues} />
        <LinkRow title="下载新版本" url={ABOUT.releases} />
        <div className="flex min-h-7 items-center justify-between gap-3">
          <span className="shrink-0 text-muted-foreground">设置文件</span>
          <button
            type="button"
            onClick={() => void revealSettings()}
            className="rounded-md px-1.5 py-0.5 text-primary hover:bg-accent"
          >
            在文件夹中显示
          </button>
        </div>
        {error && <p className="text-xs text-destructive">{error}</p>}
      </div>

      {(ABOUT.sponsor.enabled || PAYMENTS.some((p) => p.image)) && (
        <>
          <SubHeading>赞助</SubHeading>
          <Sponsor />
        </>
      )}

      <SubHeading>致谢</SubHeading>
      <ul className="flex flex-col">
        {CREDITS.map((item) => (
          <li key={item.name}>
            <button
              type="button"
              onClick={() => void commands.openUrl(item.url)}
              className="flex w-full items-center justify-between gap-2 rounded-md px-1 py-0.5 text-left hover:bg-accent"
            >
              <span className="min-w-0 truncate">
                {item.name}
                <span className="ml-1.5 text-xs text-muted-foreground">{item.note}</span>
              </span>
              <span className="shrink-0 text-xs text-muted-foreground">{item.license}</span>
            </button>
          </li>
        ))}
      </ul>
      {REFERENCES.length > 0 && (
        <>
          <p className="text-xs text-muted-foreground">参考了以下项目的做法：</p>
          <ul className="flex flex-col">
            {REFERENCES.map((item) => (
              <li key={item.name}>
                <button
                  type="button"
                  onClick={() => void commands.openUrl(item.url)}
                  className="flex w-full items-center justify-between gap-2 rounded-md px-1 py-0.5 text-left hover:bg-accent"
                >
                  <span className="min-w-0 truncate">{item.name}</span>
                  <span className="shrink-0 text-xs text-muted-foreground">{item.note}</span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      <p className="mt-1 text-xs text-muted-foreground">
        本程序以 {ABOUT.license} 许可证开源；Animate UI 部分适用其 MIT + Commons Clause
        许可证，不能单独出售或再分发。
        {ABOUT.copyright && (
          <>
            <br />
            {ABOUT.copyright}
          </>
        )}
      </p>
    </div>
  );
}

/** 设置页最后的“关于”：默认折叠，点击标题行展开。展开状态不保存。 */
export function AboutSection() {
  const [expanded, setExpanded] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  return (
    <section>
      <h2 className="px-4 pt-3 pb-1 text-xs font-medium text-muted-foreground">关于</h2>
      <div className="mr-1 ml-3 rounded-xl border border-border bg-card">
        <button
          type="button"
          aria-expanded={expanded}
          onClick={() => setExpanded((v) => !v)}
          className={cn(
            "flex min-h-14 w-full items-center justify-between gap-3 rounded-xl p-3 text-left",
            "transition-colors hover:bg-accent/40",
            "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
          )}
        >
          <span className="font-medium">关于本软件</span>
          <span className="flex items-center gap-1 text-xs text-muted-foreground">
            版本、作者、赞助与致谢
            <ChevronDown
              size={16}
              className={cn("transition-transform duration-200", expanded && "rotate-180")}
            />
          </span>
        </button>
        <AnimatePresence initial={false}>
          {expanded && (
            <motion.div
              key="body"
              ref={bodyRef}
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={EXPAND_TRANSITION}
              onAnimationStart={(target) => {
                // 与分组展开相同：先按最终高度调整窗口，窗口与内容同时动画。
                const el = bodyRef.current;
                if (!el) return;
                const collapsing =
                  typeof target === "object" && "height" in target && target.height === 0;
                const delta = collapsing ? -el.offsetHeight : el.scrollHeight - el.offsetHeight;
                if (delta !== 0) announceHeightAnimation(delta, EXPAND_TRANSITION.duration * 1000);
              }}
              className="overflow-hidden"
            >
              <div className="border-t border-border p-3">
                <AboutContent />
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </section>
  );
}
