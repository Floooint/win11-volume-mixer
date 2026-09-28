import type { ComponentProps, ReactNode } from "react";
import { AnimateIcon } from "@/components/animate-ui/icons/icon";
import { cn } from "@/lib/utils";

type IconButtonProps = Omit<ComponentProps<"button">, "children"> & {
  /** 同时作为无障碍标签和悬停提示。 */
  label: string;
  children: ReactNode;
};

/** 图标按钮。鼠标移到按钮任意位置时，按钮内的 Animate UI 图标播放动画。 */
export function IconButton({ label, className, children, ...props }: IconButtonProps) {
  return (
    <AnimateIcon animateOnHover asChild>
      <button
        type="button"
        aria-label={label}
        title={label}
        className={cn(
          "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground",
          "transition-colors hover:bg-accent hover:text-foreground",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
          className,
        )}
        {...props}
      >
        {children}
      </button>
    </AnimateIcon>
  );
}
