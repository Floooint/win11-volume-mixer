import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { MotionConfig } from "motion/react";
import { useEffect, useState } from "react";
import { MainPage } from "@/components/MainPage";
import { SettingsPage } from "@/components/SettingsPage";
import { useApplyAccent, useSystemAccent } from "@/lib/accent";
import { useAudioStore } from "@/stores/audio";
import { useSettingsStore } from "@/stores/settings";

export default function App() {
  const connect = useAudioStore((s) => s.connect);
  const loadSettings = useSettingsStore((s) => s.load);
  const [page, setPage] = useState<"main" | "settings">("main");
  const accent = useSettingsStore((s) => s.settings?.accent ?? null);
  const systemAccent = useSystemAccent();
  useApplyAccent(accent, systemAccent);

  // 在最外层订阅，切换页面时不会重复获取快照。
  useEffect(() => connect(), [connect]);
  useEffect(() => void loadSettings(), [loadSettings]);

  return (
    // 系统开启“减少动画”时，Motion 动画（列表等）只保留透明度变化。
    <MotionConfig reducedMotion="user">
      <TooltipPrimitive.Provider delayDuration={300}>
        <main className="h-screen overflow-hidden text-sm">
          {/* 以页面为 key：切换时重新挂载并播放淡入动画。 */}
          <div key={page} className="h-full motion-safe:animate-page-in">
            {page === "main" ? (
              <MainPage onOpenSettings={() => setPage("settings")} />
            ) : (
              <SettingsPage onBack={() => setPage("main")} />
            )}
          </div>
        </main>
      </TooltipPrimitive.Provider>
    </MotionConfig>
  );
}
