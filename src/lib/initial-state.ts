import type { AudioSnapshot, Settings_Serialize as Settings } from "@/bindings";

/**
 * 后端创建主窗口时注入的初始数据（src-tauri/src/window.rs `initial_state_script`），
 * 首次渲染就能显示完整列表，不必先显示骨架屏。详情浮窗没有注入，为 `null`。
 * 快照可能在音频线程忙时缺失（`null`），此时由 store 照常读取。
 */
type InitialState = {
  snapshot: AudioSnapshot | null;
  settings: Settings;
  defaults: Settings;
};

declare global {
  interface Window {
    __INITIAL_STATE__?: InitialState;
  }
}

export const initialState: InitialState | null = window.__INITIAL_STATE__ ?? null;
