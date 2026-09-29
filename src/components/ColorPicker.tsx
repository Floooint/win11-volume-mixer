import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

/** HSV（h: 0–360，s、v: 0–1）。 */
type Hsv = { h: number; s: number; v: number };

function hexToHsv(hex: string): Hsv {
  const n = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => c / 255);
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
  }
  return { h: (h * 60 + 360) % 360, s: max === 0 ? 0 : d / max, v: max };
}

function hsvToHex({ h, s, v }: Hsv): string {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return `#${[f(5), f(3), f(1)]
    .map((c) =>
      Math.round(c * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")
    .toUpperCase()}`;
}

const HEX = /^#[0-9a-fA-F]{6}$/;
const clamp01 = (x: number) => Math.min(1, Math.max(0, x));

/**
 * 按住后跟随指针拖动，每次移动以（0–1 的横向、纵向位置）调用 `onMove`，松手时调用 `onEnd`。
 * 用 pointer capture：拖出区域外仍能继续调节。
 */
function useDrag(onMove: (x: number, y: number) => void, onEnd: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  const handlers = useRef({ onMove, onEnd });
  handlers.current = { onMove, onEnd };
  const update = (e: React.PointerEvent) => {
    const rect = ref.current?.getBoundingClientRect();
    if (!rect) return;
    handlers.current.onMove(
      clamp01((e.clientX - rect.left) / rect.width),
      clamp01((e.clientY - rect.top) / rect.height),
    );
  };
  return {
    ref,
    onPointerDown: (e: React.PointerEvent) => {
      if (e.button !== 0) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      update(e);
    },
    onPointerMove: (e: React.PointerEvent) => {
      if (e.currentTarget.hasPointerCapture(e.pointerId)) update(e);
    },
    onPointerUp: (e: React.PointerEvent) => {
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      e.currentTarget.releasePointerCapture(e.pointerId);
      handlers.current.onEnd();
    },
  };
}

/**
 * 取色器：饱和度 / 亮度方块、色相条和十六进制输入框。
 * 拖动期间调用 `onPreview`（只预览，不保存），松手或输入框确认后调用 `onChange` 保存，
 * 避免拖动时每一帧都写一次设置文件。
 */
export function ColorPicker({
  value,
  onPreview,
  onChange,
}: {
  /** 当前颜色 `#RRGGBB`。 */
  value: string;
  onPreview?: (color: string) => void;
  onChange: (color: string) => void;
}) {
  const [hsv, setHsv] = useState(() => hexToHsv(value));
  const [text, setText] = useState(value);
  const color = hsvToHex(hsv);
  // 外部值变化（如恢复默认、点了预设色）时同步；拖动产生的变化与当前颜色相同，不会重置色相。
  useEffect(() => {
    if (value.toUpperCase() !== hsvToHex(hsv)) setHsv(hexToHsv(value));
    setText(value.toUpperCase());
    // 只在外部值变化时同步；hsv 仅用于比较，不作为依赖。
  }, [value]);

  const preview = (next: Hsv) => {
    setHsv(next);
    setText(hsvToHex(next));
    onPreview?.(hsvToHex(next));
  };
  const commit = () => onChange(color);

  const area = useDrag((x, y) => preview({ ...hsv, s: x, v: 1 - y }), commit);
  const hue = useDrag((x) => preview({ ...hsv, h: Math.min(359.9, x * 360) }), commit);

  return (
    <div className="mt-2 flex flex-col gap-2">
      <div
        {...area}
        role="slider"
        aria-label="饱和度与亮度"
        aria-valuenow={Math.round(hsv.s * 100)}
        className="relative h-28 cursor-crosshair touch-none rounded-md"
        style={{
          background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${hsv.h} 100% 50%))`,
        }}
      >
        <span
          className="pointer-events-none absolute size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgb(0_0_0/0.4)]"
          style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, backgroundColor: color }}
        />
      </div>
      <div
        {...hue}
        role="slider"
        aria-label="色相"
        aria-valuenow={Math.round(hsv.h)}
        className="relative h-3 cursor-pointer touch-none rounded-full"
        style={{
          background:
            "linear-gradient(to right, #f00, #ff0 17%, #0f0 33%, #0ff 50%, #00f 67%, #f0f 83%, #f00)",
        }}
      >
        <span
          className="pointer-events-none absolute top-1/2 size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgb(0_0_0/0.4)]"
          style={{ left: `${(hsv.h / 360) * 100}%`, backgroundColor: `hsl(${hsv.h} 100% 50%)` }}
        />
      </div>
      <div className="flex items-center gap-2">
        <span
          aria-hidden
          className="size-6 shrink-0 rounded-full border border-border"
          style={{ backgroundColor: color }}
        />
        <input
          aria-label="十六进制颜色"
          value={text}
          maxLength={7}
          spellCheck={false}
          onChange={(e) => {
            const next = e.target.value.startsWith("#") ? e.target.value : `#${e.target.value}`;
            setText(next.toUpperCase());
            if (HEX.test(next)) {
              setHsv(hexToHsv(next));
              onPreview?.(next.toUpperCase());
            }
          }}
          onBlur={() => (HEX.test(text) ? onChange(text) : setText(color))}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          className={cn(
            "h-7 w-24 rounded-md border border-input bg-background px-2 font-mono text-xs select-text",
            "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
          )}
        />
      </div>
    </div>
  );
}

/** “自定义”色块：彩色圆环，选中自定义颜色时中间显示该颜色。 */
export function CustomSwatch({
  selected,
  open,
  color,
  onClick,
}: {
  selected: boolean;
  open: boolean;
  color: string | null;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      aria-expanded={open}
      aria-label="自定义颜色"
      title="自定义颜色"
      onClick={onClick}
      style={{
        background: "conic-gradient(#f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)",
      }}
      className={cn(
        "relative flex size-6 items-center justify-center rounded-full ring-offset-2 ring-offset-card transition-shadow",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        selected || open ? "ring-2 ring-foreground/70" : "hover:ring-2 hover:ring-foreground/25",
      )}
    >
      {selected && color && (
        <span
          className="size-3 rounded-full border border-white/80"
          style={{ backgroundColor: color }}
        />
      )}
    </button>
  );
}
