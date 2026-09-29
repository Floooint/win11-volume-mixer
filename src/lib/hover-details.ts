/**
 * 鼠标在应用上停留一段时间后，在主窗口旁显示详情浮窗（见 src-tauri/src/details.rs）。
 * 离开应用行 1 秒后浮窗消失（由后端计时，期间移到浮窗上则保持显示）。
 * 同一时间只有一个浮窗，因此计时状态放在模块中。
 */
import { type AppDetails, commands } from "@/bindings";

/** 停留多久后显示。 */
const DELAY_MS = 1000;
/** 离开后多久消失，与后端 `details::HIDE_DELAY` 一致。 */
const HIDE_MS = 1000;

let timer: ReturnType<typeof setTimeout> | undefined;
let shown = false;
/** 最近一次离开的应用和时间：浮窗还没消失时移回同一个应用，直接保持显示。 */
let left: { appId: string; at: number } | null = null;

/** 鼠标进入应用行时调用。只响应鼠标，触摸和笔不显示。 */
export function startHoverDetails(e: React.PointerEvent<HTMLElement>, details: AppDetails) {
  if (e.pointerType !== "mouse") return;
  clearTimeout(timer);
  const row = e.currentTarget;
  const show = () => {
    // 在显示时读取位置：停留期间列表可能已经移动。
    shown = true;
    void commands.showAppDetails(details, row.getBoundingClientRect().top);
  };
  // 显示新内容时后端会取消待执行的隐藏，浮窗不会先消失再出现。
  const back = left?.appId === details.app.appId && performance.now() - left.at < HIDE_MS;
  left = null;
  if (back) show();
  else timer = setTimeout(show, DELAY_MS);
}

/**
 * 鼠标离开应用行时调用：取消计时；已显示的浮窗 1 秒后隐藏，
 * 期间鼠标移到浮窗上（选择、复制文字，点击路径）则保持显示。
 */
export function leaveHoverDetails(appId: string) {
  clearTimeout(timer);
  timer = undefined;
  if (shown) {
    shown = false;
    left = { appId, at: performance.now() };
    void commands.hideAppDetailsSoon();
  }
}

/** 在应用行上按下（如开始拖动）或滚动时调用：取消计时，已显示的浮窗立即隐藏。 */
export function cancelHoverDetails() {
  clearTimeout(timer);
  timer = undefined;
  left = null;
  if (shown) {
    shown = false;
    void commands.hideAppDetails();
  }
}
