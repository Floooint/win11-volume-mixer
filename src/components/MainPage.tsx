import { convertFileSrc } from "@tauri-apps/api/core";
import { Pin, Plus } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import {
  type ReactNode,
  type Ref,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { type AppAudio, commands, type PinMode } from "@/bindings";
import { AudioLines } from "@/components/animate-ui/icons/audio-lines";
import { Settings as SettingsIcon } from "@/components/animate-ui/icons/settings";
import { Trash2 } from "@/components/animate-ui/icons/trash-2";
import { VolumeOff } from "@/components/animate-ui/icons/volume-off";
import { IconButton } from "@/components/IconButton";
import { MainPageSkeleton, Skeleton } from "@/components/Skeleton";
import { VolumeRow } from "@/components/VolumeRow";
import { useFitWindowHeight } from "@/hooks/use-fit-window-height";
import { copyData } from "@/lib/context-copy";
import { cn } from "@/lib/utils";
import { useAudioStore } from "@/stores/audio";
import { useSettingsStore } from "@/stores/settings";
import { DEBUG_APP_PREFIX, useDebugStore } from "@/stores/debug";

/**
 * 应用图标，由后端经 `appicon` 协议提供。加载期间显示骨架占位，避免首字母一闪而过；
 * 没有图标来源或加载失败时显示首字母占位，正在发声时换成强调色。
 * 调用方以 `app.icon` 作为 key，来源变化时重置加载状态。
 */
function AppAvatar({ app }: { app: AppAudio }) {
  const [status, setStatus] = useState<"loading" | "loaded" | "error">(
    app.icon ? "loading" : "error",
  );

  if (app.icon && status !== "error") {
    return (
      <div aria-hidden className="relative flex size-8 shrink-0 items-center justify-center">
        {status === "loading" && <Skeleton className="absolute size-7 rounded-lg" />}
        <img
          src={convertFileSrc(app.icon, "appicon")}
          alt=""
          draggable={false}
          onLoad={() => setStatus("loaded")}
          onError={() => setStatus("error")}
          className={cn(
            "relative size-7 object-contain transition-opacity",
            status === "loading" && "opacity-0",
          )}
        />
      </div>
    );
  }

  const letter = app.appId === "system" ? "系" : (app.name.trim()[0] ?? "?").toUpperCase();
  return (
    <div
      aria-hidden
      className={cn(
        "flex size-8 shrink-0 items-center justify-center rounded-lg text-sm font-semibold transition-colors",
        app.active
          ? "bg-primary/15 text-primary ring-1 ring-primary/30"
          : "bg-secondary text-secondary-foreground",
      )}
    >
      {letter}
    </div>
  );
}

const PIN_MODES: Record<PinMode, { next: PinMode; label: string }> = {
  normal: { next: "pinned", label: "固定窗口：当前失焦自动隐藏，点击改为定住" },
  pinned: { next: "pinnedOnTop", label: "固定窗口：当前已定住，点击改为定住并置顶" },
  pinnedOnTop: { next: "normal", label: "固定窗口：当前已定住并置顶，点击恢复失焦隐藏" },
};

/**
 * 图钉：在“正常（失焦隐藏）”“定住”“定住并置顶”之间循环切换。
 * 状态保存在后端（只在本次运行中有效），窗口重建后重新读取。
 */
function PinButton() {
  const [mode, setMode] = useState<PinMode>("normal");
  useEffect(() => {
    commands.getPinMode().then(setMode);
  }, []);

  const { next, label } = PIN_MODES[mode];
  return (
    <IconButton
      label={label}
      aria-pressed={mode !== "normal"}
      onClick={() => {
        setMode(next);
        void commands.setPinMode(next);
      }}
      className={cn(
        mode !== "normal" && "text-primary hover:text-primary",
        mode === "pinnedOnTop" && "bg-primary/15 hover:bg-primary/20",
      )}
    >
      <Pin
        size={16}
        fill={mode === "pinnedOnTop" ? "currentColor" : "none"}
        className={cn("transition-transform duration-150", mode === "normal" && "rotate-45")}
      />
    </IconButton>
  );
}

/** 列表区域的空状态或错误状态。 */
function EmptyState({
  icon,
  title,
  hint,
  action,
}: {
  icon: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-1 px-6 py-10 text-center text-sm text-muted-foreground">
      <div className="mb-1">{icon}</div>
      <p className="text-foreground">{title}</p>
      {hint && <p className="text-xs">{hint}</p>}
      {action}
    </div>
  );
}

function ErrorToast() {
  const error = useAudioStore((s) => s.error);
  const clearError = useAudioStore((s) => s.clearError);

  useEffect(() => {
    if (!error) return;
    const timer = setTimeout(clearError, 4000);
    return () => clearTimeout(timer);
  }, [error, clearError]);

  if (!error) return null;
  return (
    <div
      role="alert"
      className="absolute inset-x-3 bottom-3 rounded-lg border border-destructive/30 bg-card px-3 py-2 text-xs text-destructive shadow-lg"
    >
      {error}
    </div>
  );
}

/** 列表动画的时长与曲线，与窗口高度动画（animation.rs 的 RESIZE）一致。 */
const LIST_TRANSITION = { duration: 0.18, ease: [0.33, 1, 0.68, 1] } as const;

/**
 * 一个应用。出现时淡入，退出时原地淡出（`popLayout` 下立即让出位置，列表高度只变化一次，
 * 由窗口高度动画过渡），排序变化时平滑移动到新位置。`ref` 由 `AnimatePresence` 使用。
 */
function AppItem({
  app,
  scrollAreaRef,
  ref,
}: {
  app: AppAudio;
  scrollAreaRef: RefObject<HTMLElement | null>;
  ref?: Ref<HTMLLIElement>;
}) {
  const setAppVolume = useAudioStore((s) => s.setAppVolume);
  const setAppMute = useAudioStore((s) => s.setAppMute);
  const debug = useDebugStore();
  const isDebug = app.appId.startsWith(DEBUG_APP_PREFIX);

  return (
    <motion.li
      ref={ref}
      // 只动画位置：用 transform 实现，不改变测得的内容高度。
      layout="position"
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={LIST_TRANSITION}
      data-copy={copyData([
        { label: "应用名", value: app.name },
        app.processName && { label: "进程名", value: app.processName },
        app.exePath && { label: "路径", value: app.exePath },
      ])}
      className={cn(
        "rounded-lg px-2 py-2 transition-colors",
        // 正在发声：浅色底，一眼就能找到。
        app.active ? "bg-primary/7 hover:bg-primary/11" : "hover:bg-accent/60",
      )}
    >
      <VolumeRow
        name={app.name}
        detail={
          [app.sessionCount > 1 ? `${app.sessionCount} 个会话` : null, isDebug ? "调试占位" : null]
            .filter(Boolean)
            .join(" · ") || undefined
        }
        volume={app.volume}
        active={app.active}
        leading={<AppAvatar key={app.icon ?? ""} app={app} />}
        scrollAreaRef={scrollAreaRef}
        trailing={
          isDebug && (
            <IconButton
              label={`移除 ${app.name}`}
              onClick={() => debug.remove(app.appId)}
              className="size-6"
            >
              <Trash2 size={13} />
            </IconButton>
          )
        }
        onVolumeChange={(v) =>
          isDebug ? debug.setVolume(app.appId, v) : setAppVolume(app.appId, v)
        }
        onMuteChange={(m) => (isDebug ? debug.setMute(app.appId, m) : setAppMute(app.appId, m))}
      />
    </motion.li>
  );
}

export function MainPage({ onOpenSettings }: { onOpenSettings: () => void }) {
  const snapshot = useAudioStore((s) => s.snapshot);
  const setMasterVolume = useAudioStore((s) => s.setMasterVolume);
  const setMasterMute = useAudioStore((s) => s.setMasterMute);
  const loadError = useAudioStore((s) => s.loadError);
  const retry = useAudioStore((s) => s.retry);
  const debugApps = useDebugStore((s) => s.apps);
  const addDebugApp = useDebugStore((s) => s.add);
  const clearDebugApps = useDebugStore((s) => s.clear);
  const debugTools = useSettingsStore((s) => s.settings?.debugTools ?? false);

  const masterAtBottom = useSettingsStore((s) => s.settings?.masterAtBottom ?? false);
  const appsReversed = useSettingsStore((s) => s.settings?.appsReversed ?? false);

  const device = snapshot?.device;
  const masterSection = device && (
    <section
      data-copy={copyData([{ label: "设备名", value: device.name }])}
      className="mx-3 rounded-xl border border-border bg-card px-3 py-3"
    >
      <VolumeRow
        name="系统音量"
        detail={device.name}
        volume={device.master}
        onVolumeChange={setMasterVolume}
        onMuteChange={setMasterMute}
        onVolumeCommit={() => void commands.playVolumeFeedback()}
      />
    </section>
  );
  const apps = [...(snapshot?.apps ?? []), ...(debugTools ? debugApps : [])];
  // 倒序：正在播放的应用排在底部，更靠近任务栏。
  if (appsReversed) apps.reverse();

  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  useFitWindowHeight(rootRef, scrollRef, contentRef);

  // 倒序时列表可滚动，默认停在底部，先看到正在播放的应用。
  const loaded = snapshot !== null;
  useLayoutEffect(() => {
    const scroll = scrollRef.current;
    if (appsReversed && loaded && scroll) scroll.scrollTop = scroll.scrollHeight;
  }, [appsReversed, loaded]);

  return (
    <div ref={rootRef} className="relative flex h-full flex-col">
      <header className="flex items-center justify-between px-4 pt-3 pb-2">
        <h1 className="min-w-0 truncate text-sm font-semibold">更优雅的音量控制器</h1>
        <div className="flex items-center gap-0.5">
          {debugTools && (
            <>
              {debugApps.length > 0 && (
                <IconButton label="清除全部调试应用" onClick={clearDebugApps}>
                  <Trash2 size={16} />
                </IconButton>
              )}
              <IconButton label="添加调试应用" onClick={addDebugApp}>
                <Plus size={16} />
              </IconButton>
            </>
          )}
          <PinButton />
          <IconButton label="设置" onClick={onOpenSettings}>
            <SettingsIcon size={16} />
          </IconButton>
        </div>
      </header>

      {/* 系统音量固定在顶部（或底部），只有应用列表滚动。 */}
      {device && !masterAtBottom && (
        <>
          {masterSection}
          <h2 className="px-4 pt-4 pb-1 text-xs font-medium text-muted-foreground">应用</h2>
        </>
      )}
      {device && masterAtBottom && (
        <h2 className="px-4 pt-1 pb-1 text-xs font-medium text-muted-foreground">应用</h2>
      )}

      {/* layoutScroll：列表滚动后，排序动画仍能算对位置。 */}
      <motion.div
        ref={scrollRef}
        layoutScroll
        className="scroll-area min-h-0 flex-1 overflow-y-auto"
      >
        <div ref={contentRef} className={device && masterAtBottom ? "pb-2" : "pb-3"}>
          {snapshot === null && loadError ? (
            <EmptyState
              icon={<VolumeOff size={24} />}
              title="无法读取音频设备"
              hint={loadError}
              action={
                <button
                  type="button"
                  onClick={() => void retry()}
                  className="mt-1 rounded-md border border-border bg-card px-3 py-1 text-foreground hover:bg-accent"
                >
                  重试
                </button>
              }
            />
          ) : snapshot === null ? (
            <MainPageSkeleton masterAtBottom={masterAtBottom} />
          ) : !device ? (
            <EmptyState
              icon={<VolumeOff size={24} />}
              title="未检测到输出设备"
              hint="连接扬声器或耳机后会自动显示"
            />
          ) : apps.length === 0 ? (
            <EmptyState
              icon={<AudioLines size={24} animateOnView loop />}
              title="暂无正在使用声音的应用"
            />
          ) : (
            // 可滚动时滚轮用于滚动列表，不调节应用音量；系统音量不受影响。
            <ul className="relative flex flex-col gap-1 pr-1 pl-3">
              {/* 首次显示不播放进入动画。 */}
              <AnimatePresence initial={false} mode="popLayout">
                {apps.map((app) => (
                  <AppItem key={app.appId} app={app} scrollAreaRef={scrollRef} />
                ))}
              </AnimatePresence>
            </ul>
          )}
        </div>
      </motion.div>

      {device && masterAtBottom && <div className="pb-3">{masterSection}</div>}

      <ErrorToast />
    </div>
  );
}
