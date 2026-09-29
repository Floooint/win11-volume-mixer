import { create } from "zustand";
import {
  type AppAudio,
  type AppError,
  type AudioSnapshot,
  commands,
  events,
  type VolumeState,
} from "@/bindings";

type AudioState = {
  snapshot: AudioSnapshot | null;
  /** 最近一次失败的提示，由界面展示后自动清除。 */
  error: string | null;
  /** 首次获取状态失败的原因。此时没有快照可显示，界面显示错误和“重试”。 */
  loadError: string | null;
  /** 获取初始状态并订阅后端事件，返回取消订阅函数。 */
  connect: () => () => void;
  /** 首次获取失败后重试。 */
  retry: () => Promise<void>;
  setMasterVolume: (volume: number) => void;
  setMasterMute: (muted: boolean) => void;
  setAppVolume: (appId: string, volume: number) => void;
  setAppMute: (appId: string, muted: boolean) => void;
  /**
   * 组音量：`apps` 为组内应用及其在组音量 100% 时的音量，按比例缩放后写入。
   * 界面中组内应用的音量同时更新。
   */
  setGroupVolume: (apps: [string, number][], volume: number) => void;
  setGroupMute: (appIds: string[], muted: boolean) => void;
  clearError: () => void;
};

type CommandResult = { status: "ok"; data: null } | { status: "error"; error: AppError };

/**
 * 按码点比较，与后端 Rust 的字符串比较一致。不能用 `localeCompare`：中文会按拼音排序，
 * 与后端快照的顺序不同，每次更新单个应用时列表都会重新排序。
 */
function compareCodePoints(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** 与后端（`aggregate.rs`）一致：活跃在前，再按名称（不分大小写）、AppId 排序。 */
function sortApps(apps: AppAudio[]): AppAudio[] {
  return [...apps].sort(
    (a, b) =>
      Number(b.active) - Number(a.active) ||
      compareCodePoints(a.name.toLowerCase(), b.name.toLowerCase()) ||
      compareCodePoints(a.appId, b.appId),
  );
}

export const useAudioStore = create<AudioState>((set, get) => {
  const update = (fn: (s: AudioSnapshot) => AudioSnapshot) =>
    set((state) => (state.snapshot ? { snapshot: fn(state.snapshot) } : state));

  const applyMaster = (patch: Partial<VolumeState>) =>
    update((s) =>
      s.device ? { ...s, device: { ...s.device, master: { ...s.device.master, ...patch } } } : s,
    );

  const applyApp = (appId: string, patch: Partial<VolumeState>) =>
    update((s) => ({
      ...s,
      apps: s.apps.map((a) =>
        a.appId === appId ? { ...a, volume: { ...a.volume, ...patch } } : a,
      ),
    }));

  const applyUpsert = (app: AppAudio) =>
    update((s) => ({
      ...s,
      apps: sortApps([...s.apps.filter((a) => a.appId !== app.appId), app]),
    }));

  const applyRemove = (appId: string) =>
    update((s) => ({ ...s, apps: s.apps.filter((a) => a.appId !== appId) }));

  const refresh = async () => {
    const result = await commands.getSnapshot();
    if (result.status === "ok") set({ snapshot: result.data, loadError: null });
  };

  const load = async () => {
    const result = await commands.getSnapshot();
    if (result.status === "ok") set({ snapshot: result.data, error: null, loadError: null });
    else set({ loadError: result.error.message });
  };

  /**
   * 乐观更新：先改本地状态让界面立即响应，再调用后端。
   * 后端自身修改不推送事件（防回跳），所以失败时需要重新获取快照恢复真实状态。
   */
  const run = async (optimistic: () => void, call: () => Promise<CommandResult>) => {
    optimistic();
    const result = await call();
    if (result.status === "error") {
      set({ error: result.error.message });
      await refresh();
    }
  };

  return {
    snapshot: null,
    error: null,
    loadError: null,
    connect: () => {
      // 先订阅再获取快照，避免两者之间的变化丢失。
      const listeners = Promise.all([
        events.audioSnapshot.listen((e) => set({ snapshot: e.payload })),
        events.audioMaster.listen((e) => applyMaster(e.payload)),
        events.audioAppUpsert.listen((e) => applyUpsert(e.payload)),
        events.audioAppRemove.listen((e) => applyRemove(e.payload.appId)),
      ]).catch((e: unknown) => {
        // 订阅失败时仍然加载快照，至少能显示当前状态和“重试”，而不是一直停在骨架屏。
        console.error("订阅音频事件失败", e);
        return [];
      });

      void listeners.then(load);

      return () => {
        listeners.then((unlisten) => unlisten.forEach((fn) => fn()));
      };
    },
    setMasterVolume: (volume) =>
      run(
        () => applyMaster({ volume }),
        () => commands.setMasterVolume(volume),
      ),
    setMasterMute: (muted) =>
      run(
        () => applyMaster({ muted }),
        () => commands.setMasterMute(muted),
      ),
    setAppVolume: (appId, volume) =>
      run(
        () => applyApp(appId, { volume }),
        () => commands.setAppVolume(appId, volume),
      ),
    setAppMute: (appId, muted) =>
      run(
        () => applyApp(appId, { muted }),
        () => commands.setAppMute(appId, muted),
      ),
    setGroupVolume: (apps, volume) =>
      run(
        () => apps.forEach(([appId, full]) => applyApp(appId, { volume: full * volume })),
        () => commands.setGroupVolume(apps, volume),
      ),
    setGroupMute: (appIds, muted) =>
      run(
        () => appIds.forEach((appId) => applyApp(appId, { muted })),
        () => commands.setGroupMute(appIds, muted),
      ),
    retry: load,
    clearError: () => {
      if (get().error) set({ error: null });
    },
  };
});
