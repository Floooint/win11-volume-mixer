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
  /** 获取初始状态并订阅后端事件，返回取消订阅函数。 */
  connect: () => () => void;
  setMasterVolume: (volume: number) => void;
  setMasterMute: (muted: boolean) => void;
  setAppVolume: (appId: string, volume: number) => void;
  setAppMute: (appId: string, muted: boolean) => void;
  clearError: () => void;
};

type CommandResult = { status: "ok"; data: null } | { status: "error"; error: AppError };

/** 与后端一致：活跃在前，再按名称、AppId 排序。 */
function sortApps(apps: AppAudio[]): AppAudio[] {
  return [...apps].sort(
    (a, b) =>
      Number(b.active) - Number(a.active) ||
      a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
      a.appId.localeCompare(b.appId),
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
      apps: s.apps.map((a) => (a.appId === appId ? { ...a, volume: { ...a.volume, ...patch } } : a)),
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
    if (result.status === "ok") set({ snapshot: result.data });
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
    connect: () => {
      // 先订阅再获取快照，避免两者之间的变化丢失。
      const listeners = Promise.all([
        events.audioSnapshot.listen((e) => set({ snapshot: e.payload })),
        events.audioMaster.listen((e) => applyMaster(e.payload)),
        events.audioAppUpsert.listen((e) => applyUpsert(e.payload)),
        events.audioAppRemove.listen((e) => applyRemove(e.payload.appId)),
      ]);

      listeners
        .then(() => commands.getSnapshot())
        .then((result) => {
          if (result.status === "ok") set({ snapshot: result.data, error: null });
          else set({ error: result.error.message });
        });

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
    clearError: () => {
      if (get().error) set({ error: null });
    },
  };
});
