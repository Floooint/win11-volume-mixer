import type { ComponentProps, ReactNode } from "react";
import { AnimateIcon } from "@/components/animate-ui/icons/icon";
import { Tooltip } from "@/components/Tooltip";
import { cn } from "@/lib/utils";

type IconButtonProps = Omit<ComponentProps<"button">, "children"> & {
  /** 同时作为无障碍标签和悬停提示（`Tooltip`）。 */
  label: string;
  children: ReactNode;
};

/**
 * 图标按钮。鼠标移到按钮任意位置时，按钮内的 Animate UI 图标播放动画，并显示提示。
 * `Tooltip` 必须在 `AnimateIcon` 内层：Animate UI 的 Slot 合并属性时，外层传入的事件会覆盖
 * 子元素的同名事件（不合并），放在外层时 Radix 的 onClick 会盖掉按钮自己的 onClick。
 */
export function IconButton({ label, className, children, ...props }: IconButtonProps) {
  return (
    <AnimateIcon animateOnHover asChild>
      <Tooltip content={label}>
        <button
          type="button"
          aria-label={label}
          className={cn(
            "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground",
            "transition-colors hover:bg-accent hover:text-foreground",
            "disabled:pointer-events-none disabled:opacity-40",
            "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
            className,
          )}
          {...props}
        >
          {children}
        </button>
      </Tooltip>
    </AnimateIcon>
  );
}
