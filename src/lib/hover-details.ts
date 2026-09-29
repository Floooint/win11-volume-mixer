/**
 * 鼠标在应用上停留一段时间后，在主窗口旁显示详情浮窗（见 src-tauri/src/details.rs）。
 * 同一时间只有一个浮窗，因此计时状态放在模块中。
 */
import { type AppDetails, commands } from "@/bindings";

/** 停留多久后显示。 */
const DELAY_MS = 3000;

let timer: ReturnType<typeof setTimeout> | undefined;
let shown = false;

/** 鼠标进入应用行时调用。只响应鼠标，触摸和笔不显示。 */
export function startHoverDetails(e: React.PointerEvent<HTMLElement>, details: AppDetails) {
  if (e.pointerType !== "mouse") return;
  cancelHoverDetails();
  const row = e.currentTarget;
  timer = setTimeout(() => {
    // 在显示时读取位置：停留期间列表可能已经移动。
    shown = true;
    void commands.showAppDetails(details, row.getBoundingClientRect().top);
  }, DELAY_MS);
}

/** 鼠标离开、按下或滚动时调用：取消计时，已显示的浮窗隐藏。 */
export function cancelHoverDetails() {
  clearTimeout(timer);
  timer = undefined;
  if (shown) {
    shown = false;
    void commands.hideAppDetails();
  }
}
