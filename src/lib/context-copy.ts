/**
 * 右键菜单的“复制…”项。右键时记下可复制的内容，后端在弹出原生菜单前读取
 * `window.__contextCopyItems` 并据此插入菜单项（见 src-tauri/src/context_menu.rs）。
 *
 * 元素可用 `data-copy={copyData([...])}` 声明额外的可复制内容，如应用行的应用名、进程名。
 */

export type CopyItem = { label: string; value: string };

declare global {
  interface Window {
    __contextCopyItems?: CopyItem[];
  }
}

/** 生成 `data-copy` 属性值，忽略空值。 */
export function copyData(items: (CopyItem | null | undefined | false | "")[]): string {
  return JSON.stringify(items.filter((item): item is CopyItem => !!item && !!item.value));
}

/** 文字过长时不提供“复制文字”，避免把大段内容塞进菜单逻辑。 */
const MAX_TEXT = 500;

export function installContextCopy() {
  // 捕获阶段执行，保证在菜单弹出前记下。
  window.addEventListener(
    "contextmenu",
    (e) => {
      const target = e.target instanceof Element ? e.target : null;
      const items: CopyItem[] = [];
      const host = target?.closest<HTMLElement>("[data-copy]");
      if (host?.dataset.copy) {
        try {
          items.push(...(JSON.parse(host.dataset.copy) as CopyItem[]));
        } catch {
          // 属性值无效时忽略，不影响菜单
        }
      }
      // 只取直接点中的文字元素（没有子元素），不把整行内容拼在一起。
      const text = target && target.children.length === 0 ? target.textContent?.trim() : "";
      if (text && text.length <= MAX_TEXT && !items.some((item) => item.value === text)) {
        items.push({ label: "文字", value: text });
      }
      window.__contextCopyItems = items;
    },
    true,
  );
}
