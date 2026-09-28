import { useEffect, useRef, useState } from "react";
import { commands, type WindowPolicy_Serialize as WindowPolicy } from "@/bindings";
import { ArrowLeft } from "@/components/animate-ui/icons/arrow-left";
import { IconButton } from "@/components/IconButton";
import { useFitWindowHeight } from "@/hooks/use-fit-window-height";
import { cn } from "@/lib/utils";
import { useSettingsStore } from "@/stores/settings";

const MODES: { value: WindowPolicy; label: string; hint: string }[] = [
  { value: "smart", label: "智能", hint: "隐藏一段时间后释放界面，兼顾速度与内存" },
  { value: "resident", label: "常驻", hint: "界面始终保留，打开最快，后台内存较高" },
  { value: "silent", label: "静默", hint: "隐藏即释放界面，内存最低，打开约慢 0.5 秒" },
];

/** 与后端 `SMART_SECONDS_RANGE` 保持一致。 */
const MIN_SECONDS = 10;
const MAX_SECONDS = 600;

/** 与后端 `animation::FPS_RANGE` 保持一致。 */
const MIN_FPS = 30;
const MAX_FPS = 240;

/**
 * 整数输入框。输入过程中允许临时的非法值，失焦或回车时再修正到范围内并保存。
 * `optional` 时允许清空：清空表示使用默认值（`placeholder` 显示默认值）。
 */
function NumberInput({
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
    if (draft.trim() === "" && optional) {
      next = null;
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

/** Win11 风格开关。 */
function Switch({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
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

/** 一项带开关的设置。 */
function SwitchRow({
  title,
  hint,
  checked,
  onChange,
}: {
  title: string;
  hint: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span>
        <span className="block font-medium">{title}</span>
        <span className="block text-xs text-muted-foreground">{hint}</span>
      </span>
      <Switch checked={checked} onChange={onChange} />
    </div>
  );
}

export function SettingsPage({ onBack }: { onBack: () => void }) {
  const settings = useSettingsStore((s) => s.settings);
  const error = useSettingsStore((s) => s.error);
  const save = useSettingsStore((s) => s.save);
  const [refreshRate, setRefreshRate] = useState<number | null>(null);
  useEffect(() => {
    commands.getRefreshRate().then(setRefreshRate);
  }, []);

  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  useFitWindowHeight(rootRef, scrollRef, contentRef);

  return (
    <div ref={rootRef} className="flex h-full flex-col">
      <header className="flex items-center gap-1 px-2 pt-3 pb-2">
        <IconButton label="返回" onClick={onBack}>
          <ArrowLeft size={16} />
        </IconButton>
        <h1 className="text-sm font-semibold">设置</h1>
      </header>

      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        <div ref={contentRef} className="pb-3">
          {settings && (
            <section className="mx-3 rounded-xl border border-border bg-card p-3">
              <p className="font-medium">运行模式</p>
              <p className="mt-0.5 text-xs text-muted-foreground">窗口隐藏后如何处理界面</p>
              <div className="mt-3 flex flex-col gap-2" role="radiogroup" aria-label="运行模式">
                {MODES.map((mode) => {
                  const selected = settings.windowPolicy === mode.value;
                  return (
                    <button
                      key={mode.value}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      onClick={() => save({ windowPolicy: mode.value })}
                      className={`rounded-md border px-3 py-2 text-left ${
                        selected ? "border-primary bg-primary/10" : "border-border hover:bg-accent"
                      }`}
                    >
                      <div className="font-medium">{mode.label}</div>
                      <div className="text-xs text-muted-foreground">{mode.hint}</div>
                    </button>
                  );
                })}
              </div>

              {settings.windowPolicy === "smart" && (
                <label className="mt-3 flex items-center justify-between gap-2 text-sm">
                  <span>
                    释放等待时间
                    <span className="block text-xs text-muted-foreground">
                      {MIN_SECONDS}–{MAX_SECONDS} 秒
                    </span>
                  </span>
                  <span className="flex items-center gap-1.5">
                    <NumberInput
                      label="释放等待时间（秒）"
                      value={settings.smartReleaseSeconds}
                      min={MIN_SECONDS}
                      max={MAX_SECONDS}
                      step={10}
                      onCommit={(seconds) =>
                        seconds !== null && save({ smartReleaseSeconds: seconds })
                      }
                    />
                    秒
                  </span>
                </label>
              )}
              {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
            </section>
          )}
          {settings && (
            <section className="mx-3 mt-3 flex flex-col gap-3 rounded-xl border border-border bg-card p-3">
              <SwitchRow
                title="音量提示音"
                hint="调节系统音量后播放提示音，声音大小即当前音量"
                checked={settings.volumeFeedback}
                onChange={(checked) => save({ volumeFeedback: checked })}
              />
              <div className="flex items-center justify-between gap-3">
                <span>
                  <span className="block font-medium">动画帧率</span>
                  <span className="block text-xs text-muted-foreground">
                    {settings.animationFps === null
                      ? `跟随显示器刷新率${refreshRate ? `（${refreshRate} Hz）` : ""}`
                      : `${MIN_FPS}–${MAX_FPS} 帧，清空则跟随显示器`}
                  </span>
                </span>
                <span className="flex items-center gap-1.5">
                  <NumberInput
                    label="动画帧率（帧 / 秒）"
                    value={settings.animationFps}
                    min={MIN_FPS}
                    max={MAX_FPS}
                    step={1}
                    optional
                    placeholder={refreshRate ? String(refreshRate) : "自动"}
                    onCommit={(fps) => save({ animationFps: fps })}
                  />
                  帧
                </span>
              </div>
              <SwitchRow
                title="调试工具"
                hint="在主界面显示“添加占位应用”按钮，用于测试多应用布局"
                checked={settings.debugTools}
                onChange={(checked) => save({ debugTools: checked })}
              />
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
