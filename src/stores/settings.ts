import { create } from "zustand";
import { commands, type Settings_Serialize as Settings } from "@/bindings";

type SettingsState = {
  settings: Settings | null;
  /** 各项默认值，用于判断是否显示“恢复默认”。 */
  defaults: Settings | null;
  error: string | null;
  load: () => Promise<void>;
  /** 乐观保存：先更新界面，失败时回滚并显示错误。 */
  save: (patch: Partial<Settings>) => Promise<void>;
};

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: null,
  defaults: null,
  error: null,
  load: async () => {
    const [settings, defaults] = await Promise.all([
      commands.getSettings(),
      commands.getDefaultSettings(),
    ]);
    set({ settings, defaults });
  },
  save: async (patch) => {
    const previous = get().settings;
    if (!previous) return;
    const next = { ...previous, ...patch };
    set({ settings: next });
    const result = await commands.setSettings(next);
    if (result.status === "error") {
      set({ settings: previous, error: result.error.message });
    } else {
      set({ error: null });
    }
  },
}));
