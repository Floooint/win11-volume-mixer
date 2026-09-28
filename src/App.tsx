import { useEffect, useState } from "react";
import { events } from "@/bindings";
import { useAudioStore } from "@/stores/audio";

const percent = (volume: number) => `${Math.round(volume * 100)}%`;

// 阶段 2 调试视图：验证命令与事件已接通。正式的音量界面在阶段 3 实现。
export default function App() {
  const snapshot = useAudioStore((s) => s.snapshot);
  const error = useAudioStore((s) => s.error);
  const connect = useAudioStore((s) => s.connect);
  const [log, setLog] = useState<string[]>([]);

  useEffect(() => connect(), [connect]);

  // 事件日志只用于调试，与状态更新无关。
  useEffect(() => {
    const push = (line: string) =>
      setLog((prev) => [`${new Date().toLocaleTimeString()} ${line}`, ...prev].slice(0, 30));
    const listeners = Promise.all([
      events.audioSnapshot.listen((e) => push(`snapshot：${e.payload.device?.name ?? "无设备"}`)),
      events.audioMaster.listen((e) => push(`系统音量：${percent(e.payload.volume)}`)),
      events.audioAppUpsert.listen((e) =>
        push(`应用音量：${e.payload.name} ${percent(e.payload.volume.volume)}`),
      ),
      events.audioAppRemove.listen((e) => push(`应用退出：${e.payload.appId}`)),
    ]);
    return () => {
      listeners.then((unlisten) => unlisten.forEach((fn) => fn()));
    };
  }, []);

  return (
    <main className="flex h-screen flex-col gap-3 overflow-hidden p-4 text-sm">
      <h1 className="text-base font-semibold">Win11 声音控制器 · 调试视图</h1>
      {error && <p className="text-destructive">{error}</p>}
      {snapshot && (
        <section className="rounded-lg bg-card p-3">
          <p className="font-medium">
            {snapshot.device
              ? `${snapshot.device.name} · 系统音量 ${percent(snapshot.device.master.volume)}${snapshot.device.master.muted ? "（静音）" : ""}`
              : "当前没有输出设备"}
          </p>
          <p className="mt-2 text-xs text-muted-foreground">应用音量（相对于系统音量）：</p>
          <ul className="mt-1 space-y-1 text-muted-foreground">
            {snapshot.apps.map((app) => (
              <li key={app.appId}>
                {app.active ? "▶ " : "  "}
                {app.name} · {percent(app.volume.volume)}
                {app.volume.muted && "（静音）"}
              </li>
            ))}
          </ul>
        </section>
      )}
      <section className="min-h-0 flex-1 overflow-auto rounded-lg bg-card p-3 font-mono text-xs text-muted-foreground">
        {log.length === 0
          ? "等待事件……在音量合成器中调节音量试试"
          : log.map((line, i) => <div key={i}>{line}</div>)}
      </section>
    </main>
  );
}
