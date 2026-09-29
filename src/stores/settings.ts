import { create } from "zustand";
import { commands, type SavedApp, type Settings_Serialize as Settings } from "@/bindings";

type SettingsState = {
  settings: Settings | null;
  /** 各项默认值，用于判断是否显示“恢复默认”。 */
  defaults: Settings | null;
  error: string | null;
  load: () => Promise<void>;
  /** 乐观保存：先更新界面，失败时回滚并显示错误。 */
  save: (patch: Partial<Settings>) => Promise<void>;
  /** 置顶应用，排在已置顶应用之后；同时取消隐藏。 */
  pinApp: (app: SavedApp) => Promise<void>;
  unpinApp: (appId: string) => Promise<void>;
  /** 按新顺序排列置顶应用（拖拽排序）。 */
  reorderPinned: (appIds: string[]) => Promise<void>;
  /** 隐藏应用；同时取消置顶。 */
  hideApp: (app: SavedApp) => Promise<void>;
  unhideApp: (appId: string) => Promise<void>;
};

const without = (apps: SavedApp[], appId: string) => apps.filter((a) => a.appId !== appId);

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
  pinApp: async (app) => {
    const s = get().settings;
    if (!s) return;
    await get().save({
      pinnedApps: [...without(s.pinnedApps, app.appId), app],
      hiddenApps: without(s.hiddenApps, app.appId),
    });
  },
  unpinApp: async (appId) => {
    const s = get().settings;
    if (s) await get().save({ pinnedApps: without(s.pinnedApps, appId) });
  },
  reorderPinned: async (appIds) => {
    const s = get().settings;
    if (!s) return;
    const byId = new Map(s.pinnedApps.map((a) => [a.appId, a]));
    const ordered = appIds.flatMap((id) => byId.get(id) ?? []);
    // 不在新顺序中的（当前没在运行、没显示出来的）置顶应用保留在末尾。
    const rest = s.pinnedApps.filter((a) => !appIds.includes(a.appId));
    await get().save({ pinnedApps: [...ordered, ...rest] });
  },
  hideApp: async (app) => {
    const s = get().settings;
    if (!s) return;
    await get().save({
      hiddenApps: [...without(s.hiddenApps, app.appId), app],
      pinnedApps: without(s.pinnedApps, app.appId),
    });
  },
  unhideApp: async (appId) => {
    const s = get().settings;
    if (s) await get().save({ hiddenApps: without(s.hiddenApps, appId) });
  },
}));
