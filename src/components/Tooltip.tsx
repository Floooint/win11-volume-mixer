import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import {
  type ComponentProps,
  type ReactElement,
  type ReactNode,
  type Ref,
  useRef,
  useState,
} from "react";
import { cn } from "@/lib/utils";

type TooltipProps = Omit<
  ComponentProps<typeof TooltipPrimitive.Trigger>,
  "content" | "children"
> & {
  /** 提示内容；可用 `\n` 换行。为空时只渲染子元素。 */
  content: ReactNode;
  side?: ComponentProps<typeof TooltipPrimitive.Content>["side"];
  /** 只在触发元素的文字被截断（`truncate`）时显示，用于显示完整名称。 */
  onlyWhenTruncated?: boolean;
  /** 触发元素，须能接收 ref 和事件（DOM 元素或转发它们的组件）。 */
  children: ReactElement;
};

/**
 * 悬停或聚焦时显示的提示，替代原生 `title`（样式与窗口一致，窄窗口中也不会被裁切）。
 * 需要外层有 `TooltipPrimitive.Provider`（见 App 和 main.tsx 中的详情浮窗）。
 *
 * 其余属性转给触发元素：可作为 Slot 的子元素（如 `AnimateIcon asChild` 内），
 * 外层传入的事件与子元素自身的事件由 Radix 合并，不会互相覆盖。
 */
export function Tooltip({
  content,
  side = "top",
  onlyWhenTruncated = false,
  children,
  ref,
  ...props
}: TooltipProps) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLElement | null>(null);
  if (content === null || content === undefined || content === "") return children;

  const setRef = (node: HTMLButtonElement | null) => {
    trigger.current = node;
    assignRef(ref, node);
  };
  // 文字没被截断时完整内容已经可见，不打开。
  const truncated = () => {
    const el = trigger.current;
    return !!el && el.scrollWidth > el.clientWidth;
  };

  return (
    <TooltipPrimitive.Root
      open={open}
      onOpenChange={(next) => setOpen(next && (!onlyWhenTruncated || truncated()))}
    >
      <TooltipPrimitive.Trigger asChild ref={setRef} {...props}>
        {children}
      </TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={4}
          collisionPadding={8}
          className={cn(
            "z-50 max-w-[min(18rem,calc(100vw-16px))] rounded-md border border-border bg-popover px-2.5 py-1.5",
            "text-xs wrap-break-word whitespace-pre-line text-popover-foreground shadow-md animate-pop-in",
          )}
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

function assignRef<T>(ref: Ref<T> | undefined, value: T) {
  if (typeof ref === "function") ref(value);
  else if (ref) ref.current = value;
}
