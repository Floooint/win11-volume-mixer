import * as SelectPrimitive from "@radix-ui/react-select";
import * as SliderPrimitive from "@radix-ui/react-slider";
import { Check, ChevronDown } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { RotateCcw } from "@/components/animate-ui/icons/rotate-ccw";
import { IconButton } from "@/components/IconButton";
import { Tooltip } from "@/components/Tooltip";
import { ignoreNonPrimary } from "@/components/VolumeSlider";
import { cn } from "@/lib/utils";

/** 标题右上角的问号，悬停或聚焦时显示说明。 */
function Help({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Tooltip content={children}>
      <button
        type="button"
        aria-label={`${label}说明`}
        className={cn(
          // 上标样式的问号，颜色较浅，不抢标题的注意力；悬停时加深。
          "-mt-1 inline-flex size-3.5 items-center justify-center rounded-sm",
          "text-[11px] leading-none font-semibold text-muted-foreground/50",
          "transition-colors hover:text-muted-foreground",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
        )}
      >
        ?
      </button>
    </Tooltip>
  );
}

/**
 * 一项设置：左侧标题（带说明问号），右侧控件。值不是默认值时，控件左侧出现“恢复默认”按钮。
 * `below` 放在标题行下方，如滑块。
 */
export function SettingRow({
  title,
  help,
  isDefault = true,
  onReset,
  children,
  below,
}: {
  title: string;
  help?: ReactNode;
  isDefault?: boolean;
  onReset?: () => void;
  children?: ReactNode;
  below?: ReactNode;
}) {
  return (
    <div>
      <div className="flex min-h-8 items-center justify-between gap-3">
        <span className="flex items-start gap-0.5 font-medium">
          {title}
          {help && <Help label={title}>{help}</Help>}
        </span>
        <span className="flex items-center gap-1">
          {!isDefault && onReset && (
            <IconButton label={`将“${title}”恢复默认`} onClick={onReset} className="size-7">
              <RotateCcw size={14} />
            </IconButton>
          )}
          {children}
        </span>
      </div>
      {below}
    </div>
  );
}

/** Win11 风格开关。 */
export function Switch({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-5 w-10 shrink-0 items-center rounded-full border transition-colors",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
        checked ? "border-primary bg-primary" : "border-muted-foreground/60 bg-transparent",
      )}
    >
      <span
        className={cn(
          "absolute size-3 rounded-full transition-all duration-150 ease-out",
          checked ? "left-5.5 bg-primary-foreground" : "left-0.75 bg-muted-foreground",
        )}
      />
    </button>
  );
}

/**
 * 整数输入框。输入过程中允许临时的非法值，失焦或回车时再修正到范围内并保存。
 * `optional` 时允许清空：清空表示使用默认值（`placeholder` 显示默认值）。
 */
export function NumberInput({
  value,
  min,
  max,
  step,
  label,
  placeholder,
  optional = false,
  onCommit,
}: {
  value: number | null;
  min: number;
  max: number;
  step: number;
  label: string;
  placeholder?: string;
  optional?: boolean;
  onCommit: (value: number | null) => void;
}) {
  const text = (v: number | null) => (v === null ? "" : String(v));
  const [draft, setDraft] = useState(text(value));
  useEffect(() => setDraft(text(value)), [value]);

  const commit = () => {
    let next: number | null;
    if (draft.trim() === "") {
      // 清空：可选项表示“不设置”，否则恢复原值（`Number("")` 为 0，会被当成最小值）。
      next = optional ? null : value;
    } else {
      const parsed = Number(draft);
      next = Number.isFinite(parsed) ? Math.min(max, Math.max(min, Math.round(parsed))) : value;
    }
    setDraft(text(next));
    if (next !== value) onCommit(next);
  };

  return (
    <input
      type="number"
      inputMode="numeric"
      aria-label={label}
      min={min}
      max={max}
      step={step}
      value={draft}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      className="w-20 rounded-md border border-input bg-background px-2 py-1 text-right text-foreground placeholder:text-muted-foreground"
    />
  );
}

export type SelectOption<T extends string> = { value: T; label: string };

/** 下拉选择。展开的列表与按钮同宽。 */
export function Select<T extends string>({
  value,
  options,
  label,
  onChange,
}: {
  value: T;
  options: SelectOption<T>[];
  label: string;
  onChange: (value: T) => void;
}) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={(v) => onChange(v as T)}>
      <SelectPrimitive.Trigger
        aria-label={label}
        className={cn(
          "inline-flex h-8 min-w-24 shrink-0 items-center justify-between gap-2 rounded-md border border-input bg-background px-2.5 whitespace-nowrap",
          "transition-colors hover:bg-accent/60",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
        )}
      >
        <SelectPrimitive.Value />
        <SelectPrimitive.Icon>
          <ChevronDown size={14} className="text-muted-foreground" />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          side="bottom"
          align="end"
          sideOffset={4}
          collisionPadding={8}
          className={cn(
            "z-50 min-w-(--radix-select-trigger-width) rounded-lg border border-border bg-popover p-1",
            "text-popover-foreground shadow-lg animate-pop-in",
          )}
        >
          <SelectPrimitive.Viewport>
            {options.map((option) => (
              <SelectPrimitive.Item
                key={option.value}
                value={option.value}
                className={cn(
                  "relative flex cursor-default items-center rounded-md py-1.5 pr-2 pl-7 outline-none select-none",
                  "data-highlighted:bg-accent",
                )}
              >
                <SelectPrimitive.ItemIndicator className="absolute left-2 flex">
                  <Check size={14} className="text-primary" />
                </SelectPrimitive.ItemIndicator>
                <SelectPrimitive.ItemText>{option.label}</SelectPrimitive.ItemText>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

/** 设置页滑块，外观与音量滑块一致。拖动时 `onChange`，松手（或键盘松开）时 `onCommit`。 */
export function Slider({
  value,
  min,
  max,
  step,
  label,
  onChange,
  onCommit,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  label: string;
  onChange: (value: number) => void;
  onCommit: (value: number) => void;
}) {
  return (
    <SliderPrimitive.Root
      className="relative flex h-7 w-full cursor-pointer touch-none items-center select-none"
      min={min}
      max={max}
      step={step}
      value={[value]}
      aria-label={label}
      onPointerDown={ignoreNonPrimary}
      onValueChange={([v]) => onChange(v)}
      onValueCommit={([v]) => onCommit(v)}
    >
      <SliderPrimitive.Track className="relative h-1 grow overflow-hidden rounded-full bg-muted-foreground/25">
        <SliderPrimitive.Range className="absolute h-full rounded-full bg-primary" />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb
        className={cn(
          "relative block size-4 rounded-full border-4 border-primary bg-background shadow-sm",
          "transition-[border-width] duration-150 hover:border-[3px] active:border-[5px]",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
          "after:absolute after:-inset-2 after:rounded-full after:content-['']",
        )}
      />
    </SliderPrimitive.Root>
  );
}
