import { create } from "zustand";
import type { AppAudio } from "@/bindings";

const SAMPLE_NAMES = [
  "Google Chrome",
  "Spotify",
  "Microsoft Teams",
  "Visual Studio Code",
  "Discord",
  "Steam",
  "OBS Studio",
  "VLC media player",
  "网易云音乐",
  "哔哩哔哩",
  "QQ 音乐",
  "微信",
];

/** 调试用 AppId 前缀，便于和真实应用区分。 */
export const DEBUG_APP_PREFIX = "debug:";

type DebugState = {
  apps: AppAudio[];
  add: () => void;
  remove: (appId: string) => void;
  clear: () => void;
  setVolume: (appId: string, volume: number) => void;
  setMute: (appId: string, muted: boolean) => void;
};

let counter = 0;

/**
 * 调试用占位应用，用于在没有足够真实应用时检查多应用布局、滚动和窗口高度。
 * 只存在于前端，不会调用后端，也不会影响系统音量。
 */
export const useDebugStore = create<DebugState>((set) => {
  const patch = (appId: string, fn: (app: AppAudio) => AppAudio) =>
    set((s) => ({ apps: s.apps.map((a) => (a.appId === appId ? fn(a) : a)) }));

  return {
    apps: [],
    add: () => {
      counter += 1;
      const name = SAMPLE_NAMES[(counter - 1) % SAMPLE_NAMES.length];
      const app: AppAudio = {
        appId: `${DEBUG_APP_PREFIX}${counter}`,
        name:
          counter > SAMPLE_NAMES.length
            ? `${name} ${Math.ceil(counter / SAMPLE_NAMES.length)}`
            : name,
        icon: null,
        volume: { volume: Math.round(Math.random() * 100) / 100, muted: false },
        // 交替出现活跃 / 不活跃，便于对比两种样式。
        active: counter % 2 === 1,
        sessionCount: counter % 3 === 0 ? 2 : 1,
      };
      set((s) => ({ apps: [...s.apps, app] }));
    },
    remove: (appId) => set((s) => ({ apps: s.apps.filter((a) => a.appId !== appId) })),
    clear: () => set({ apps: [] }),
    setVolume: (appId, volume) => patch(appId, (a) => ({ ...a, volume: { ...a.volume, volume } })),
    setMute: (appId, muted) => patch(appId, (a) => ({ ...a, volume: { ...a.volume, muted } })),
  };
});
