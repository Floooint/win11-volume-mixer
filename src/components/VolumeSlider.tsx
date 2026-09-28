import * as SliderPrimitive from "@radix-ui/react-slider";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

/** 拖动时向后端发送的最小间隔，见 docs/architecture.md“Commands”。 */
const SEND_INTERVAL_MS = 30;

type VolumeSliderProps = {
  /** 当前音量，0–1。 */
  value: number;
  /** 拖动或键盘调节时调用，已节流。 */
  onChange: (value: number) => void;
  muted?: boolean;
  label: string;
  className?: string;
};

/**
 * 音量滑块。拖动期间以本地值为准，忽略外部传入的 `value`，
 * 避免后端推送的旧值让滑块回跳；松手后再与外部值同步。
 */
export function VolumeSlider({ value, onChange, muted, label, className }: VolumeSliderProps) {
  const [local, setLocal] = useState(value);
  const dragging = useRef(false);
  const lastSent = useRef(0);
  const trailing = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!dragging.current) setLocal(value);
  }, [value]);

  useEffect(() => () => {
    if (trailing.current) clearTimeout(trailing.current);
  }, []);

  const send = (next: number) => {
    if (trailing.current) clearTimeout(trailing.current);
    const wait = SEND_INTERVAL_MS - (Date.now() - lastSent.current);
    if (wait <= 0) {
      lastSent.current = Date.now();
      onChange(next);
    } else {
      // 节流窗口内的最后一个值延后发送，保证最终值一定送达。
      trailing.current = setTimeout(() => {
        lastSent.current = Date.now();
        onChange(next);
      }, wait);
    }
  };

  return (
    <SliderPrimitive.Root
      className={cn(
        // 高度即可点击范围：比轨道高得多，便于点中；与旁边的静音按钮等高。
        "relative flex h-7 w-full cursor-pointer touch-none items-center select-none",
        muted && "opacity-50",
        className,
      )}
      min={0}
      max={100}
      step={1}
      value={[Math.round(local * 100)]}
      aria-label={label}
      onPointerDown={() => {
        dragging.current = true;
      }}
      onValueChange={([v]) => {
        const next = v / 100;
        setLocal(next);
        send(next);
      }}
      onValueCommit={() => {
        dragging.current = false;
      }}
    >
      <SliderPrimitive.Track className="relative h-1 grow overflow-hidden rounded-full bg-muted-foreground/25">
        <SliderPrimitive.Range className="absolute h-full rounded-full bg-primary" />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb
        className={cn(
          "relative block size-4 rounded-full border-4 border-primary bg-background shadow-sm",
          "transition-[border-width] duration-150 hover:border-[3px] active:border-[5px]",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
          // 不可见的扩展区域，拖柄实际可抓取范围为 32×32 px。
          "after:absolute after:-inset-2 after:rounded-full after:content-['']",
        )}
      />
    </SliderPrimitive.Root>
  );
}
