import { type RefObject, useEffect } from "react";
import { commands } from "@/bindings";

/** 当前页面的 `announceHeightAnimation` 处理函数（同一时间只有一个页面使用本 hook）。 */
let announceHandler: ((delta: number, durationMs: number) => void) | null = null;

/**
 * 内容即将以动画改变高度（如展开 / 折叠分组）时调用，`delta` 为高度变化量（px）。
 * 立即按最终高度调整窗口，窗口动画与内容动画同时进行（两者时长、曲线相同）；
 * 否则每帧的高度变化都会重新发起窗口动画，窗口一直被打断、落后于内容。
 */
export function announceHeightAnimation(delta: number, durationMs: number) {
  announceHandler?.(delta, durationMs);
}

/**
 * 让窗口高度跟随内容：窗口高度 = 页面固定部分 + 滚动区内容的完整高度。
 * 后端把高度限制在屏幕的 4/5 以内，超出时滚动区出现滚动条。
 * 窗口可见时后端以动画调整高度；动画期间窗口可能暂时比内容矮，因此隐藏滚动条，结束后恢复。
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
    let resizeEnd: ReturnType<typeof setTimeout> | undefined;
    const endResize = () => {
      clearTimeout(resizeEnd);
      scroll.style.overflowY = "";
    };
    // 必须用带小数的尺寸：clientHeight 各自取整后相减，固定部分会随窗口高度差 1 px，
    // 调整窗口后测得的高度又变回原值，窗口在两个高度之间来回跳动。
    const contentHeight = () =>
      root.getBoundingClientRect().height -
      scroll.getBoundingClientRect().height +
      content.getBoundingClientRect().height;
    // 内容高度动画期间不逐帧测量，结束后再测一次校正。
    let holdUntil = 0;
    let holdEnd: ReturnType<typeof setTimeout> | undefined;

    const measure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (performance.now() < holdUntil) return;
        send(Math.ceil(contentHeight()));
      });
    };
    const send = (height: number) => {
      if (height !== lastSent) {
        lastSent = height;
        clearTimeout(resizeEnd);
        scroll.style.overflowY = "hidden";
        void commands.fitWindowHeight(height).then(
          (ms) => {
            // 期间又发起了新的调整时，由新的调整负责恢复。
            if (lastSent !== height) return;
            if (ms > 0) resizeEnd = setTimeout(endResize, ms);
            else endResize();
          },
          // 调整失败时也要恢复滚动条，并允许之后再次发送同一高度。
          () => {
            if (lastSent === height) lastSent = 0;
            endResize();
          },
        );
      }
    };
    announceHandler = (delta, durationMs) => {
      holdUntil = performance.now() + durationMs;
      send(Math.ceil(contentHeight() + delta));
      clearTimeout(holdEnd);
      holdEnd = setTimeout(measure, durationMs + 20);
    };

    // 内容变化（应用增减）、窗口尺寸变化，以及固定部分高度变化（如首次运行的询问关闭，
    // 此时只有滚动区变高，根元素和内容都不变）都需要重新测量。
    const observer = new ResizeObserver(measure);
    observer.observe(content);
    observer.observe(root);
    observer.observe(scroll);
    measure();

    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(holdEnd);
      observer.disconnect();
      announceHandler = null;
      endResize();
    };
  }, [rootRef, scrollRef, contentRef]);
}
