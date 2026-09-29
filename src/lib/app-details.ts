/**
 * 点击应用行，在主窗口旁显示详情浮窗（见 src-tauri/src/details.rs）；再次点击同一应用、
 * 点击主窗口其他位置、列表滚动或主窗口失去焦点时消失。是否正在显示由后端判断。
 */
import { type AppDetails, commands } from "@/bindings";

/** 应用行上的这些控件有自己的点击操作，点击它们不切换详情。音量滑块整行标记了 `data-no-details`。 */
const CONTROLS = "button, input, textarea, a, [role='slider'], [data-no-details]";

/** 点击位置是应用行上切换详情的区域（行内空白、名称、图标等），返回所在的应用行。 */
function detailsRow(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof Element) || target.closest(CONTROLS)) return null;
  return target.closest<HTMLElement>("[data-details-row]");
}

/** 应用行的点击事件：点在切换区域时显示或隐藏这个应用的详情。 */
export function toggleAppDetails(e: React.MouseEvent<HTMLElement>, details: AppDetails) {
  // 分组内的应用行嵌在分组行中，只由最内层的应用行处理。
  if (detailsRow(e.target) !== e.currentTarget) return;
  // 选择了文字（如拖选应用名）时不算点击。
  if (window.getSelection()?.isCollapsed === false) return;
  void commands.toggleAppDetails(details, e.currentTarget.getBoundingClientRect().top);
}

/** 隐藏详情浮窗（开始拖动、切换页面等）。 */
export function closeAppDetails() {
  void commands.hideAppDetails();
}

/** 在主窗口中按下鼠标时调用：不在切换区域（由点击事件处理）就隐藏详情。 */
export function closeAppDetailsOnPointerDown(e: PointerEvent) {
  if (!detailsRow(e.target)) closeAppDetails();
}
