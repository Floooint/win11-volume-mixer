import { AudioLines } from "@/components/animate-ui/icons/audio-lines";
import { Settings as SettingsIcon } from "@/components/animate-ui/icons/settings";
import { useEffect } from "react";
import type { AppAudio } from "@/bindings";
import { IconButton } from "@/components/IconButton";
import { VolumeRow } from "@/components/VolumeRow";
import { useAudioStore } from "@/stores/audio";

/** 应用首字母占位图标（图标提取尚未实现）。 */
function AppAvatar({ app }: { app: AppAudio }) {
  const letter = app.appId === "system" ? "系" : (app.name.trim()[0] ?? "?").toUpperCase();
  return (
    <div
      aria-hidden
      className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-secondary text-sm font-semibold text-secondary-foreground"
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

export function MainPage({ onOpenSettings }: { onOpenSettings: () => void }) {
  const snapshot = useAudioStore((s) => s.snapshot);
  const setMasterVolume = useAudioStore((s) => s.setMasterVolume);
  const setMasterMute = useAudioStore((s) => s.setMasterMute);
  const setAppVolume = useAudioStore((s) => s.setAppVolume);
  const setAppMute = useAudioStore((s) => s.setAppMute);

  const device = snapshot?.device;
  const apps = snapshot?.apps ?? [];

  return (
    <div className="relative flex h-full flex-col">
      <header className="flex items-center justify-between px-4 pt-3 pb-2">
        <h1 className="text-sm font-semibold">音量</h1>
        <IconButton label="设置" onClick={onOpenSettings}>
          <SettingsIcon size={16} />
        </IconButton>
      </header>

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
            />
          </section>

          <h2 className="px-4 pt-4 pb-1 text-xs font-medium text-muted-foreground">应用</h2>
          <section className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
            {apps.length === 0 ? (
              <div className="flex flex-col items-center gap-2 py-10 text-center text-sm text-muted-foreground">
                <AudioLines size={24} animateOnView loop />
                <p>暂无正在使用声音的应用</p>
              </div>
            ) : (
              <ul className="flex flex-col gap-1">
                {apps.map((app) => (
                  <li key={app.appId} className="rounded-lg px-1 py-2 transition-colors hover:bg-accent/60">
                    <VolumeRow
                      name={app.name}
                      detail={app.sessionCount > 1 ? `${app.sessionCount} 个会话` : undefined}
                      volume={app.volume}
                      dimmed={!app.active}
                      leading={<AppAvatar app={app} />}
                      onVolumeChange={(v) => setAppVolume(app.appId, v)}
                      onMuteChange={(m) => setAppMute(app.appId, m)}
                    />
                  </li>
                ))}
              </ul>
            )}
            <p className="px-1 pt-3 text-[11px] text-muted-foreground/80">
              应用音量为相对于系统音量的百分比
            </p>
          </section>
        </>
      ) : (
        <p className="px-4 py-8 text-center text-sm text-muted-foreground">未检测到输出设备</p>
      )}

      <ErrorToast />
    </div>
  );
}
