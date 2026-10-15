import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useRef } from "react";
import { announceHeightAnimation } from "@/hooks/use-fit-window-height";

/** 与主界面列表动画（MainPage 的 `LIST_TRANSITION`）、窗口高度动画（animation.rs 的 RESIZE）一致。 */
const TRANSITION = { duration: 0.18, ease: [0.33, 1, 0.68, 1] } as const;

/**
 * 展开 / 折叠的内容：高度从 0 到自然高度，窗口高度同时以动画调整（做法同主界面的分组）。
 * 在浮层中使用时（不影响窗口高度）传 `fitWindow={false}`。
 */
export function Expand({
  open,
  fitWindow = true,
  children,
}: {
  open: boolean;
  fitWindow?: boolean;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  return (
    <AnimatePresence initial={false}>
      {open && (
        <motion.div
          key="content"
          ref={ref}
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={TRANSITION}
          onAnimationStart={(target) => {
            // 按动画目标判断方向：折叠时退出动画用的是最后一次渲染的闭包，open 仍为 true。
            const el = ref.current;
            if (!el || !fitWindow) return;
            const collapsing =
              typeof target === "object" && "height" in target && target.height === 0;
            const delta = collapsing ? -el.offsetHeight : el.scrollHeight - el.offsetHeight;
            if (delta !== 0) announceHeightAnimation(delta, TRANSITION.duration * 1000);
          }}
          className="overflow-hidden"
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  );
}
