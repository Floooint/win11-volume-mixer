import { useEffect, useRef } from "react";
import { type AppAudio, commands } from "@/bindings";
import { AudioLines } from "@/components/animate-ui/icons/audio-lines";
import { Plus } from "@/components/animate-ui/icons/plus";
import { Settings as SettingsIcon } from "@/components/animate-ui/icons/settings";
import { Trash2 } from "@/components/animate-ui/icons/trash-2";
import { IconButton } from "@/components/IconButton";
import { VolumeRow } from "@/components/VolumeRow";
import { useFitWindowHeight } from "@/hooks/use-fit-window-height";
import { cn } from "@/lib/utils";
import { useAudioStore } from "@/stores/audio";
import { useSettingsStore } from "@/stores/settings";
import { DEBUG_APP_PREFIX, useDebugStore } from "@/stores/debug";

/** 应用首字母占位图标（图标提取尚未实现）。正在发声时换成强调色。 */
function AppAvatar({ app }: { app: AppAudio }) {
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

function AppItem({ app }: { app: AppAudio }) {
  const setAppVolume = useAudioStore((s) => s.setAppVolume);
  const setAppMute = useAudioStore((s) => s.setAppMute);
  const debug = useDebugStore();
  const isDebug = app.appId.startsWith(DEBUG_APP_PREFIX);

  return (
    <li
      className={cn(
        "relative rounded-lg px-2 py-2 transition-colors",
        // 正在发声：浅色底 + 左侧强调条，一眼就能找到。
        app.active ? "bg-primary/7 hover:bg-primary/11" : "hover:bg-accent/60",
      )}
    >
      {app.active && (
        <span
          aria-hidden
          className="absolute top-2.5 bottom-2.5 left-0 w-0.75 rounded-full bg-primary"
        />
      )}
      <VolumeRow
        name={app.name}
        detail={
          [app.sessionCount > 1 ? `${app.sessionCount} 个会话` : null, isDebug ? "调试占位" : null]
            .filter(Boolean)
            .join(" · ") || undefined
        }
        volume={app.volume}
        active={app.active}
        leading={<AppAvatar app={app} />}
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
    </li>
  );
}

export function MainPage({ onOpenSettings }: { onOpenSettings: () => void }) {
  const snapshot = useAudioStore((s) => s.snapshot);
  const setMasterVolume = useAudioStore((s) => s.setMasterVolume);
  const setMasterMute = useAudioStore((s) => s.setMasterMute);
  const debugApps = useDebugStore((s) => s.apps);
  const addDebugApp = useDebugStore((s) => s.add);
  const clearDebugApps = useDebugStore((s) => s.clear);
  const debugTools = useSettingsStore((s) => s.settings?.debugTools ?? false);

  const device = snapshot?.device;
  const apps = [...(snapshot?.apps ?? []), ...(debugTools ? debugApps : [])];

  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  useFitWindowHeight(rootRef, scrollRef, contentRef);

  return (
    <div ref={rootRef} className="relative flex h-full flex-col">
      <header className="flex items-center justify-between px-4 pt-3 pb-2">
        <h1 className="text-sm font-semibold">音量</h1>
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
          <IconButton label="设置" onClick={onOpenSettings}>
            <SettingsIcon size={16} />
          </IconButton>
        </div>
      </header>

      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        <div ref={contentRef} className="pb-3">
          {snapshot === null ? (
            <p className="px-4 py-8 text-center text-sm text-muted-foreground">正在读取音频设备…</p>
          ) : device ? (
            <>
              <section className="mx-3 rounded-xl border border-border bg-card px-3 py-3">
                <VolumeRow
                  name="系统音量"
                  detail={device.name}
                  volume={device.master}
                  onVolumeChange={setMasterVolume}
                  onMuteChange={setMasterMute}
                  onVolumeCommit={() => void commands.playVolumeFeedback()}
                />
              </section>

              <h2 className="px-4 pt-4 pb-1 text-xs font-medium text-muted-foreground">应用</h2>
              <section className="px-3">
                {apps.length === 0 ? (
                  <div className="flex flex-col items-center gap-2 py-10 text-center text-sm text-muted-foreground">
                    <AudioLines size={24} animateOnView loop />
                    <p>暂无正在使用声音的应用</p>
                  </div>
                ) : (
                  <ul className="flex flex-col gap-1">
                    {apps.map((app) => (
                      <AppItem key={app.appId} app={app} />
                    ))}
                  </ul>
                )}
              </section>
            </>
          ) : (
            <p className="px-4 py-8 text-center text-sm text-muted-foreground">未检测到输出设备</p>
          )}
        </div>
      </div>

      <ErrorToast />
    </div>
  );
}
