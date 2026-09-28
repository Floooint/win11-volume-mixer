import { useEffect, useState } from "react";
import { MainPage } from "@/components/MainPage";
import { SettingsPage } from "@/components/SettingsPage";
import { useAudioStore } from "@/stores/audio";
import { useSettingsStore } from "@/stores/settings";

export default function App() {
  const connect = useAudioStore((s) => s.connect);
  const loadSettings = useSettingsStore((s) => s.load);
  const [page, setPage] = useState<"main" | "settings">("main");

  // 在最外层订阅，切换页面时不会重复获取快照。
  useEffect(() => connect(), [connect]);
  useEffect(() => void loadSettings(), [loadSettings]);

  return (
    <main className="h-screen overflow-hidden text-sm">
      {page === "main" ? (
        <MainPage onOpenSettings={() => setPage("settings")} />
      ) : (
        <SettingsPage onBack={() => setPage("main")} />
      )}
    </main>
  );
}
