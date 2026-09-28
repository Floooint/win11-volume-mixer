import { useEffect, useState } from "react";
import { type AudioSnapshot, commands, events } from "@/bindings";

// 阶段 2 调试视图：验证命令与事件已接通。正式的音量界面在阶段 3 实现。
export default function App() {
  const [snapshot, setSnapshot] = useState<AudioSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);

  useEffect(() => {
    const push = (line: string) =>
      setLog((prev) => [`${new Date().toLocaleTimeString()} ${line}`, ...prev].slice(0, 30));

    commands.getSnapshot().then((result) => {
      if (result.status === "ok") setSnapshot(result.data);
      else setError(result.error.message);
    });

    const listeners = Promise.all([
      events.audioSnapshot.listen((e) => {
        setSnapshot(e.payload);
        push(`snapshot：${e.payload.device?.name ?? "无设备"}`);
      }),
      events.audioMaster.listen((e) => push(`master：${Math.round(e.payload.volume * 100)}%`)),
      events.audioAppUpsert.listen((e) =>
        push(`upsert：${e.payload.name} ${Math.round(e.payload.volume.volume * 100)}%`),
      ),
      events.audioAppRemove.listen((e) => push(`remove：${e.payload.appId}`)),
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
              ? `${snapshot.device.name} · ${Math.round(snapshot.device.master.volume * 100)}%`
              : "当前没有输出设备"}
          </p>
          <ul className="mt-2 space-y-1 text-muted-foreground">
            {snapshot.apps.map((app) => (
              <li key={app.appId}>
                {app.active ? "▶ " : "  "}
                {app.name} · {Math.round(app.volume.volume * 100)}%
                {app.volume.muted && "（静音）"}
              </li>
            ))}
          </ul>
        </section>
      )}
      <section className="min-h-0 flex-1 overflow-auto rounded-lg bg-card p-3 font-mono text-xs text-muted-foreground">
        {log.length === 0 ? "等待事件……在音量合成器中调节音量试试" : log.map((l, i) => <div key={i}>{l}</div>)}
      </section>
    </main>
  );
}
