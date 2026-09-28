import { create } from "zustand";
import { type AppAudio, type AudioSnapshot, commands, events, type VolumeState } from "@/bindings";

type AudioState = {
  snapshot: AudioSnapshot | null;
  error: string | null;
  /** 获取初始状态并订阅后端事件，返回取消订阅函数。 */
  connect: () => () => void;
};

/** 与后端一致：活跃在前，再按名称、AppId 排序。 */
function sortApps(apps: AppAudio[]): AppAudio[] {
  return [...apps].sort(
    (a, b) =>
      Number(b.active) - Number(a.active) ||
      a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
      a.appId.localeCompare(b.appId),
  );
}

export const useAudioStore = create<AudioState>((set) => {
  const update = (fn: (s: AudioSnapshot) => AudioSnapshot) =>
    set((state) => (state.snapshot ? { snapshot: fn(state.snapshot) } : state));

  const applyMaster = (master: VolumeState) =>
    update((s) => (s.device ? { ...s, device: { ...s.device, master } } : s));

  const applyUpsert = (app: AppAudio) =>
    update((s) => ({
      ...s,
      apps: sortApps([...s.apps.filter((a) => a.appId !== app.appId), app]),
    }));

  const applyRemove = (appId: string) =>
    update((s) => ({ ...s, apps: s.apps.filter((a) => a.appId !== appId) }));

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
  };
});
