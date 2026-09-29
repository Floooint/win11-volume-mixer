/**
 * 右键菜单中由前端提供的项（复制、置顶、隐藏等）。右键时记下菜单项，后端在弹出原生菜单前读取
 * `window.__contextMenuItems` 并插入（见 src-tauri/src/context_menu.rs）。
 * 动作项被点击后，后端调用 `window.__contextMenuAction(action)`，由 `onMenuAction` 注册的处理函数执行。
 *
 * 元素可用 `data-menu={menuData([...])}` 声明菜单项，如应用行的“复制应用名”“置顶”。
 */

export type MenuItem =
  /** 点击后把 `value` 复制到剪贴板。 */
  | { label: string; value: string }
  /** 点击后执行动作，如 `pin:<appId>`。 */
  | { label: string; action: string }
  | { separator: true };

declare global {
  interface Window {
    __contextMenuItems?: MenuItem[];
    __contextMenuAction?: (action: string) => void;
  }
}

/** 生成 `data-menu` 属性值，忽略空值。 */
export function menuData(items: (MenuItem | null | undefined | false | "")[]): string {
  return JSON.stringify(items.filter((item): item is MenuItem => !!item));
}

/** 注册动作处理函数，返回取消注册的函数。 */
export function onMenuAction(handler: (action: string) => void): () => void {
  window.__contextMenuAction = handler;
  return () => {
    if (window.__contextMenuAction === handler) window.__contextMenuAction = undefined;
  };
}

/** 文字过长时不提供“复制文字”。 */
const MAX_TEXT = 500;

export function installContextMenu() {
  // 捕获阶段执行，保证在菜单弹出前记下。
  window.addEventListener(
    "contextmenu",
    (e) => {
      const target = e.target instanceof Element ? e.target : null;
      let items: MenuItem[] = [];
      const host = target?.closest<HTMLElement>("[data-menu]");
      if (host?.dataset.menu) {
        try {
          items = JSON.parse(host.dataset.menu) as MenuItem[];
        } catch {
          // 属性值无效时忽略，不影响菜单
        }
      }
      // 只取直接点中的文字元素（没有子元素），不把整行内容拼在一起。
      const text = target && target.children.length === 0 ? target.textContent?.trim() : "";
      const duplicate = items.some((item) => "value" in item && item.value === text);
      if (text && text.length <= MAX_TEXT && !duplicate) {
        items = [{ label: "复制文字", value: text }, ...items];
      }
      window.__contextMenuItems = items;
    },
    true,
  );
}
