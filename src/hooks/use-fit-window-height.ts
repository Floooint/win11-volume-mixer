import { type RefObject, useEffect } from "react";
import { commands } from "@/bindings";

/**
 * 让窗口高度跟随内容：窗口高度 = 页面固定部分 + 滚动区内容的完整高度。
 * 后端把高度限制在屏幕的 4/5 以内，超出时滚动区出现滚动条。
 *
 * @param rootRef    页面根元素（高度为 100%）
 * @param scrollRef  可滚动区域（`flex-1 min-h-0 overflow-y-auto`）
 * @param contentRef 滚动区内唯一的内容容器（内边距放在它上面），其高度即内容完整高度
 */
export function useFitWindowHeight(
  rootRef: RefObject<HTMLElement | null>,
  scrollRef: RefObject<HTMLElement | null>,
  contentRef: RefObject<HTMLElement | null>,
) {
  useEffect(() => {
    const root = rootRef.current;
    const scroll = scrollRef.current;
    const content = contentRef.current;
    if (!root || !scroll || !content) return;

    let frame = 0;
    let lastSent = 0;
    const measure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        // 必须用带小数的尺寸：clientHeight 各自取整后相减，固定部分会随窗口高度差 1 px，
        // 调整窗口后测得的高度又变回原值，窗口在两个高度之间来回跳动。
        const fixed = root.getBoundingClientRect().height - scroll.getBoundingClientRect().height;
        const height = Math.ceil(fixed + content.getBoundingClientRect().height);
        if (height !== lastSent) {
          lastSent = height;
          void commands.fitWindowHeight(height);
        }
      });
    };

    // 内容变化（应用增减）和窗口尺寸变化（固定部分高度）都需要重新测量。
    const observer = new ResizeObserver(measure);
    observer.observe(content);
    observer.observe(root);
    measure();

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [rootRef, scrollRef, contentRef]);
}
