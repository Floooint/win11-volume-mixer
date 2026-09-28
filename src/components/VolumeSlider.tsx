import * as SliderPrimitive from "@radix-ui/react-slider";
import { type RefObject, useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

/** 拖动时向后端发送的最小间隔，见 docs/architecture.md“Commands”。 */
const SEND_INTERVAL_MS = 30;

/** 滚轮每格调整的音量（0–1），与系统音量面板一致为 2%。 */
const WHEEL_STEP = 0.02;

/** 滚轮停止多久后视为一次调节结束。 */
const WHEEL_END_MS = 250;

type VolumeSliderProps = {
  /** 当前音量，0–1。 */
  value: number;
  /** 拖动、滚轮或键盘调节时调用，已节流。 */
  onChange: (value: number) => void;
  /** 一次调节结束时调用：松开拖动、滚轮停止、键盘松开。 */
  onCommit?: () => void;
  muted?: boolean;
  label: string;
  /** 响应滚轮的区域，默认为滑块本身。传入整行可让鼠标在行内任意位置滚动调节。 */
  wheelAreaRef?: RefObject<HTMLElement | null>;
  className?: string;
};

/**
 * 音量滑块。拖动或滚轮调节期间以本地值为准，忽略外部传入的 `value`，
 * 避免后端推送的旧值让滑块回跳；调节结束后再与外部值同步。
 */
export function VolumeSlider({
  value,
  onChange,
  onCommit,
  muted,
  label,
  wheelAreaRef,
  className,
}: VolumeSliderProps) {
  const [local, setLocal] = useState(value);
  const localRef = useRef(value);
  const adjusting = useRef(false);
  const lastSent = useRef(0);
  const trailing = useRef<ReturnType<typeof setTimeout> | null>(null);
  const wheelEnd = useRef<ReturnType<typeof setTimeout> | null>(null);
  const rootRef = useRef<HTMLSpanElement>(null);

  // 回调放进 ref，滚轮监听只注册一次也能拿到最新的回调。
  const onChangeRef = useRef(onChange);
  const onCommitRef = useRef(onCommit);
  onChangeRef.current = onChange;
  onCommitRef.current = onCommit;

  useEffect(() => {
    if (!adjusting.current) {
      setLocal(value);
      localRef.current = value;
    }
  }, [value]);

  useEffect(
    () => () => {
      if (trailing.current) clearTimeout(trailing.current);
      if (wheelEnd.current) clearTimeout(wheelEnd.current);
    },
    [],
  );

  const send = (next: number) => {
    if (trailing.current) clearTimeout(trailing.current);
    const wait = SEND_INTERVAL_MS - (Date.now() - lastSent.current);
    if (wait <= 0) {
      lastSent.current = Date.now();
      onChangeRef.current(next);
    } else {
      // 节流窗口内的最后一个值延后发送，保证最终值一定送达。
      trailing.current = setTimeout(() => {
        lastSent.current = Date.now();
        onChangeRef.current(next);
      }, wait);
    }
  };

  const update = (next: number) => {
    const clamped = Math.min(1, Math.max(0, Math.round(next * 100) / 100));
    if (clamped === localRef.current) return;
    localRef.current = clamped;
    setLocal(clamped);
    send(clamped);
  };

  // 滚轮：向上增大、向下减小。须用非被动监听才能阻止外层列表同时滚动。
  useEffect(() => {
    const root = wheelAreaRef?.current ?? rootRef.current;
    if (!root) return;
    const onWheel = (e: WheelEvent) => {
      if (e.deltaY === 0) return;
      e.preventDefault();
      adjusting.current = true;
      update(localRef.current + (e.deltaY < 0 ? WHEEL_STEP : -WHEEL_STEP));
      if (wheelEnd.current) clearTimeout(wheelEnd.current);
      wheelEnd.current = setTimeout(() => {
        adjusting.current = false;
        onCommitRef.current?.();
      }, WHEEL_END_MS);
    };
    root.addEventListener("wheel", onWheel, { passive: false });
    return () => root.removeEventListener("wheel", onWheel);
    // update 只读 ref，无需作为依赖。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wheelAreaRef]);

  return (
    <SliderPrimitive.Root
      ref={rootRef}
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
        adjusting.current = true;
      }}
      onValueChange={([v]) => update(v / 100)}
      onValueCommit={() => {
        adjusting.current = false;
        onCommitRef.current?.();
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
