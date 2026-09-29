import { create } from "zustand";
import {
  type AppAlias,
  type AppGroup,
  commands,
  type GroupMember,
  type SavedApp,
  type Scene,
  type SceneApp,
  type Settings_Serialize as Settings,
} from "@/bindings";

type SettingsState = {
  settings: Settings | null;
  /** 各项默认值，用于判断是否显示“恢复默认”。 */
  defaults: Settings | null;
  error: string | null;
  load: () => Promise<void>;
  /** 乐观保存：先更新界面，失败时回滚并显示错误。 */
  save: (patch: Partial<Settings>) => Promise<void>;
  /** 只更新界面、不写入设置文件，如拖动取色器时预览颜色；松手后再 `save`。 */
  preview: (patch: Partial<Settings>) => void;
  /** 置顶应用，排在已置顶应用之后；同时取消隐藏。 */
  pinApp: (app: SavedApp) => Promise<void>;
  unpinApp: (appId: string) => Promise<void>;
  /** 按新顺序排列置顶应用（拖拽排序）。 */
  reorderPinned: (appIds: string[]) => Promise<void>;
  /** 隐藏应用；同时取消置顶。 */
  hideApp: (app: SavedApp) => Promise<void>;
  unhideApp: (appId: string) => Promise<void>;
  /** 新建一个空分组（展开状态），返回其标识。 */
  createGroup: () => Promise<string>;
  /**
   * 把应用放入分组（离开原来的分组、取消置顶）。`volume` 为应用当前音量，
   * 换算为它在组音量 100% 时的音量，保证放入后实际音量不变。
   */
  addToGroup: (groupId: string, app: SavedApp, volume: number) => Promise<void>;
  removeFromGroup: (appId: string) => Promise<void>;
  /** 组音量变化后记下，组内应用的“100% 时音量”不变。 */
  updateGroup: (groupId: string, patch: Partial<Omit<AppGroup, "id" | "apps">>) => Promise<void>;
  /** 单独调节组内某个应用：`volume` 为它的新实际音量，换算后记下。 */
  setMemberVolume: (groupId: string, appId: string, volume: number) => Promise<void>;
  deleteGroup: (groupId: string) => Promise<void>;
  /** 重命名应用；`alias` 为空时恢复原名。 */
  renameApp: (app: SavedApp, alias: string | null) => Promise<void>;
  /** 把当前各应用的音量保存为新场景，返回其标识。 */
  createScene: (apps: SceneApp[]) => Promise<string>;
  updateScene: (sceneId: string, patch: Partial<Omit<Scene, "id">>) => Promise<void>;
  deleteScene: (sceneId: string) => Promise<void>;
  /**
   * 应用场景后，分组中成员的音量被直接改变：按组音量换算，更新它们在组音量 100% 时的音量，
   * 之后拖动组音量时仍按比例缩放。`volumes` 为（应用标识，新音量）。
   */
  syncGroupMembers: (volumes: [string, number][]) => Promise<void>;
};

const without = <T extends { appId: string }>(apps: T[], appId: string) =>
  apps.filter((a) => a.appId !== appId);

/** 新分组 / 场景的默认名称：“分组 1”“分组 2”…，跳过已使用的编号。 */
function nextName(items: { name: string }[], prefix: string): string {
  const used = new Set(items.map((g) => g.name));
  for (let i = 1; ; i++) {
    if (!used.has(`${prefix} ${i}`)) return `${prefix} ${i}`;
  }
}

/** 实际音量换算为组音量 100% 时的音量。组音量为 0 时无法换算，按 100%。 */
function fullVolume(volume: number, groupVolume: number): number {
  return groupVolume > 0 ? Math.min(1, volume / groupVolume) : 1;
}

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
  preview: (patch) => {
    const current = get().settings;
    if (current) set({ settings: { ...current, ...patch } });
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
  createGroup: async () => {
    const id = `group-${Date.now().toString(36)}`;
    const s = get().settings;
    if (!s) return id;
    const group: AppGroup = {
      id,
      name: nextName(s.groups, "分组"),
      apps: [],
      volume: 1,
      expanded: true,
    };
    await get().save({ groups: [...s.groups, group] });
    return id;
  },
  addToGroup: async (groupId, app, volume) => {
    const s = get().settings;
    const target = s?.groups.find((g) => g.id === groupId);
    if (!s || !target) return;
    const member: GroupMember = {
      appId: app.appId,
      name: app.name,
      fullVolume: fullVolume(volume, target.volume),
    };
    await get().save({
      groups: s.groups.map((g) =>
        g.id === groupId
          ? { ...g, apps: [...without(g.apps, app.appId), member] }
          : { ...g, apps: without(g.apps, app.appId) },
      ),
      // 分组后在组内显示，不再单独置顶。
      pinnedApps: without(s.pinnedApps, app.appId),
    });
  },
  removeFromGroup: async (appId) => {
    const s = get().settings;
    if (!s) return;
    await get().save({ groups: s.groups.map((g) => ({ ...g, apps: without(g.apps, appId) })) });
  },
  updateGroup: async (groupId, patch) => {
    const s = get().settings;
    if (!s) return;
    await get().save({ groups: s.groups.map((g) => (g.id === groupId ? { ...g, ...patch } : g)) });
  },
  setMemberVolume: async (groupId, appId, volume) => {
    const s = get().settings;
    if (!s) return;
    await get().save({
      groups: s.groups.map((g) =>
        g.id === groupId
          ? {
              ...g,
              apps: g.apps.map((a) =>
                a.appId === appId ? { ...a, fullVolume: fullVolume(volume, g.volume) } : a,
              ),
            }
          : g,
      ),
    });
  },
  deleteGroup: async (groupId) => {
    const s = get().settings;
    if (!s) return;
    await get().save({ groups: s.groups.filter((g) => g.id !== groupId) });
  },
  renameApp: async (app, alias) => {
    const s = get().settings;
    if (!s) return;
    const rest = without(s.appAliases, app.appId);
    const trimmed = alias?.trim();
    // 与原名相同视为恢复原名。
    const entry: AppAlias | null =
      trimmed && trimmed !== app.name ? { appId: app.appId, name: app.name, alias: trimmed } : null;
    await get().save({ appAliases: entry ? [...rest, entry] : rest });
  },
  createScene: async (apps) => {
    const id = `scene-${Date.now().toString(36)}`;
    const s = get().settings;
    if (!s) return id;
    const scene: Scene = { id, name: nextName(s.scenes, "场景"), apps };
    await get().save({ scenes: [...s.scenes, scene] });
    return id;
  },
  updateScene: async (sceneId, patch) => {
    const s = get().settings;
    if (!s) return;
    await get().save({ scenes: s.scenes.map((c) => (c.id === sceneId ? { ...c, ...patch } : c)) });
  },
  deleteScene: async (sceneId) => {
    const s = get().settings;
    if (!s) return;
    await get().save({ scenes: s.scenes.filter((c) => c.id !== sceneId) });
  },
  syncGroupMembers: async (volumes) => {
    const s = get().settings;
    if (!s) return;
    const byId = new Map(volumes);
    const touched = s.groups.some((g) => g.apps.some((a) => byId.has(a.appId)));
    if (!touched) return;
    await get().save({
      groups: s.groups.map((g) => ({
        ...g,
        apps: g.apps.map((a) => {
          const volume = byId.get(a.appId);
          return volume === undefined ? a : { ...a, fullVolume: fullVolume(volume, g.volume) };
        }),
      })),
    });
  },
}));
