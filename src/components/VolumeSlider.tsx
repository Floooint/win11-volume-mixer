import * as SliderPrimitive from "@radix-ui/react-slider";
import { type PointerEvent, type RefObject, useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

/** 拖动时向后端发送的最小间隔，见 docs/architecture.md“Commands”。 */
const SEND_INTERVAL_MS = 30;

/** 滚轮每格调整的音量（0–1），与系统音量面板一致为 2%。 */
const WHEEL_STEP = 0.02;

/** 滚轮停止多久后视为一次调节结束。 */
const WHEEL_END_MS = 250;

/**
 * 只有主键（左键、触摸、笔）能拖动滑块，避免想用右键打开菜单时误调数值。
 * 在滑块的 `onPointerDown` 中调用：非主键时 `preventDefault()`，
 * Radix 据此跳过自身的拖动处理；右键菜单不受影响。返回是否已忽略。
 */
export function ignoreNonPrimary(e: PointerEvent): boolean {
  if (e.button === 0) return false;
  e.preventDefault();
  return true;
}

type VolumeSliderProps = {
  /** 当前音量，0–1。 */
  value: number;
  /** 拖动、滚轮或键盘调节时调用，已节流。 */
  onChange: (value: number) => void;
  /** 一次调节结束时调用，参数为最终音量：松开拖动、滚轮停止、键盘松开。数值没有变化时不调用。 */
  onCommit?: (value: number) => void;
  muted?: boolean;
  label: string;
  /** 响应滚轮的区域，默认为滑块本身。传入整行可让鼠标在行内任意位置滚动调节。 */
  wheelAreaRef?: RefObject<HTMLElement | null>;
  /** 所在的滚动列表。列表可以滚动时，滚轮让给列表滚动，不再调节音量。 */
  scrollAreaRef?: RefObject<HTMLElement | null>;
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
  scrollAreaRef,
  className,
}: VolumeSliderProps) {
  const [local, setLocal] = useState(value);
  const localRef = useRef(value);
  const adjusting = useRef(false);
  /** 本次调节改变过数值，结束时需要提交。 */
  const dirty = useRef(false);
  const lastSent = useRef(0);
  const trailing = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** 节流中尚未发送的值。 */
  const pending = useRef<number | null>(null);
  /** 调节结束时递增，让下面的同步重新执行：调节期间被忽略的外部值此时补上。 */
  const [sync, setSync] = useState(0);
  const wheelEnd = useRef<ReturnType<typeof setTimeout> | null>(null);
  const rootRef = useRef<HTMLSpanElement>(null);

  // 回调放进 ref，滚轮监听只注册一次也能拿到最新的回调。
  const onChangeRef = useRef(onChange);
  const onCommitRef = useRef(onCommit);
  onChangeRef.current = onChange;
  onCommitRef.current = onCommit;

  // 依赖 sync：调节结束后即使外部值没再变化，也重新同步一次。
  useEffect(() => {
    if (!adjusting.current) {
      setLocal(value);
      localRef.current = value;
    }
  }, [value, sync]);

  useEffect(
    () => () => {
      if (trailing.current) clearTimeout(trailing.current);
      if (wheelEnd.current) clearTimeout(wheelEnd.current);
    },
    [],
  );

  const emit = (next: number) => {
    pending.current = null;
    lastSent.current = Date.now();
    onChangeRef.current(next);
  };

  const send = (next: number) => {
    if (trailing.current) clearTimeout(trailing.current);
    trailing.current = null;
    const wait = SEND_INTERVAL_MS - (Date.now() - lastSent.current);
    if (wait <= 0) {
      emit(next);
    } else {
      // 节流窗口内的最后一个值延后发送，保证最终值一定送达。
      pending.current = next;
      trailing.current = setTimeout(() => {
        trailing.current = null;
        emit(next);
      }, wait);
    }
  };

  const update = (next: number) => {
    const clamped = Math.min(1, Math.max(0, Math.round(next * 100) / 100));
    if (clamped === localRef.current) return;
    localRef.current = clamped;
    dirty.current = true;
    setLocal(clamped);
    send(clamped);
  };

  /**
   * 一次调节结束：先立即发出节流中待发的值，再提交最终值（否则提交时最终值可能还没发出），
   * 然后恢复与外部值同步。可以重复调用：数值没变过时不提交。只读 ref，滚轮监听中也可调用。
   */
  const finish = () => {
    if (trailing.current) {
      clearTimeout(trailing.current);
      trailing.current = null;
    }
    if (pending.current !== null) emit(pending.current);
    adjusting.current = false;
    setSync((n) => n + 1);
    if (dirty.current) {
      dirty.current = false;
      onCommitRef.current?.(localRef.current);
    }
  };

  // 滚轮：向上增大、向下减小。须用非被动监听才能阻止外层列表同时滚动。
  useEffect(() => {
    const root = wheelAreaRef?.current ?? rootRef.current;
    if (!root) return;
    const onWheel = (e: WheelEvent) => {
      if (e.deltaY === 0) return;
      // 每次都重新判断，应用增减后立即生效。
      const scroll = scrollAreaRef?.current;
      if (scroll && scroll.scrollHeight > scroll.clientHeight) return;
      e.preventDefault();
      adjusting.current = true;
      update(localRef.current + (e.deltaY < 0 ? WHEEL_STEP : -WHEEL_STEP));
      if (wheelEnd.current) clearTimeout(wheelEnd.current);
      wheelEnd.current = setTimeout(finish, WHEEL_END_MS);
    };
    root.addEventListener("wheel", onWheel, { passive: false });
    return () => root.removeEventListener("wheel", onWheel);
    // update、finish 只读 ref，无需作为依赖。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wheelAreaRef, scrollAreaRef]);

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
      // 只有拖柄带 role="slider"，点在轨道上时也不应切换应用详情（见 src/lib/app-details.ts）。
      data-no-details
      onPointerDown={(e) => {
        if (ignoreNonPrimary(e)) return;
        adjusting.current = true;
      }}
      // Radix 只在数值变化时调用 onValueCommit：按下后没拖动、拖回原值，或拖动中窗口失焦
      // 丢失指针捕获时都不会调用，因此松开和丢失捕获时也要结束调节，否则之后一直忽略外部音量。
      onPointerUp={finish}
      onLostPointerCapture={finish}
      onValueChange={([v]) => update(v / 100)}
      onValueCommit={finish}
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
