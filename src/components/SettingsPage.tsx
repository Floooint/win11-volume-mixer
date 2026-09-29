import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  commands,
  type ThemeMode,
  type WindowPolicy_Serialize as WindowPolicy,
} from "@/bindings";
import { ArrowLeft } from "@/components/animate-ui/icons/arrow-left";
import { IconButton } from "@/components/IconButton";
import {
  NumberInput,
  Select,
  type SelectOption,
  SettingRow,
  Slider,
  Switch,
} from "@/components/SettingControls";
import { useFitWindowHeight } from "@/hooks/use-fit-window-height";
import { ACCENT_PRESETS, useSystemAccent } from "@/lib/accent";
import { cn } from "@/lib/utils";
import { useSettingsStore } from "@/stores/settings";

const MODES: (SelectOption<WindowPolicy> & { hint: string })[] = [
  { value: "smart", label: "智能", hint: "隐藏一段时间后释放界面，兼顾速度与内存" },
  { value: "resident", label: "常驻", hint: "界面始终保留，打开最快，后台内存较高" },
  { value: "silent", label: "静默", hint: "隐藏即释放界面，内存最低，打开约慢 0.5 秒" },
];

/** 运行模式的说明：总述加每种模式一行。 */
const MODES_HELP = (
  <>
    <p>窗口隐藏后如何处理界面：</p>
    {MODES.map((mode) => (
      <p key={mode.value}>
        <span className="font-medium">{mode.label}</span>：{mode.hint}
      </p>
    ))}
  </>
);

const THEMES: SelectOption<ThemeMode>[] = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

/** 强调色：第一个色块为“跟随系统”，其余为预设色。 */
function AccentPicker({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (accent: string | null) => void;
}) {
  const system = useSystemAccent();
  const swatches = [
    { value: null, label: "跟随系统", color: system?.light ?? "#005FB8" },
    ...ACCENT_PRESETS.map((preset) => ({ ...preset, color: preset.value })),
  ];
  return (
    <div role="radiogroup" aria-label="强调色" className="mt-1.5 flex flex-wrap gap-2">
      {swatches.map((swatch) => {
        const selected = value === swatch.value;
        return (
          <button
            key={swatch.label}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={swatch.label}
            title={swatch.label}
            onClick={() => onChange(swatch.value)}
            style={{ backgroundColor: swatch.color }}
            className={cn(
              "relative size-6 rounded-full ring-offset-2 ring-offset-card transition-shadow",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              selected ? "ring-2 ring-foreground/70" : "hover:ring-2 hover:ring-foreground/25",
            )}
          >
            {/* “跟随系统”用字母 A 标出（Auto），与预设色区分。 */}
            {swatch.value === null && (
              <span className="absolute inset-0 flex items-center justify-center text-[10px] font-semibold text-white">
                A
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}

/** 与后端 `SMART_SECONDS_RANGE` 保持一致。 */
const MIN_SECONDS = 10;
const MAX_SECONDS = 600;

/** 与后端 `animation::FPS_RANGE` 保持一致。 */
const MIN_FPS = 30;
const MAX_FPS = 240;

/** 与后端 `config::WIDTH_RANGE` 保持一致。 */
const MIN_WIDTH = 280;
const MAX_WIDTH = 500;
const WIDTH_STEP = 10;

/**
 * 窗口宽度：拖动时实时预览（不保存），松手后保存。
 * 键盘调节同样先预览，按键松开时 Radix 触发 `onValueCommit` 保存。
 */
function WidthSetting({
  value,
  defaultValue,
  onCommit,
}: {
  value: number;
  defaultValue: number | undefined;
  onCommit: (width: number) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);

  return (
    <SettingRow
      title="窗口宽度"
      isDefault={defaultValue === undefined || draft === defaultValue}
      onReset={() => defaultValue !== undefined && onCommit(defaultValue)}
      below={
        <Slider
          label="窗口宽度"
          value={draft}
          min={MIN_WIDTH}
          max={MAX_WIDTH}
          step={WIDTH_STEP}
          onChange={(width) => {
            setDraft(width);
            void commands.previewWindowWidth(width);
          }}
          onCommit={onCommit}
        />
      }
    >
      <span className="text-xs tabular-nums text-muted-foreground">{draft} px</span>
    </SettingRow>
  );
}

/**
 * 开机自启。状态直接读写系统中的注册，不经过设置文件。默认开启（首次运行时由后端开启）。
 */
function AutostartSetting() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    commands.getAutostart().then(setEnabled);
  }, []);

  const change = async (next: boolean) => {
    setEnabled(next);
    const result = await commands.setAutostart(next);
    if (result.status === "error") {
      setError(result.error.message);
      setEnabled(await commands.getAutostart());
    } else {
      setError(null);
    }
  };

  if (enabled === null) return null;
  return (
    <SettingRow
      title="开机自启"
      help="登录 Windows 后自动启动到托盘，不弹出窗口"
      isDefault={enabled}
      onReset={() => void change(true)}
      below={error && <p className="text-xs text-destructive">{error}</p>}
    >
      <Switch label="开机自启" checked={enabled} onChange={(next) => void change(next)} />
    </SettingRow>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="px-4 pt-3 pb-1 text-xs font-medium text-muted-foreground">{title}</h2>
      <div className="mr-1 ml-3 flex flex-col gap-2 rounded-xl border border-border bg-card p-3">
        {children}
      </div>
    </section>
  );
}

export function SettingsPage({ onBack }: { onBack: () => void }) {
  const settings = useSettingsStore((s) => s.settings);
  const defaults = useSettingsStore((s) => s.defaults);
  const error = useSettingsStore((s) => s.error);
  const save = useSettingsStore((s) => s.save);
  const [refreshRate, setRefreshRate] = useState<number | null>(null);
  /** 本次在设置页改过“硬件加速”，提示需要重启。 */
  const [restartNeeded, setRestartNeeded] = useState(false);
  useEffect(() => {
    commands.getRefreshRate().then(setRefreshRate);
  }, []);

  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  useFitWindowHeight(rootRef, scrollRef, contentRef);

  /** 某项是否为默认值；默认值尚未读取时视为默认，不显示“恢复默认”。 */
  const isDefault = <K extends keyof NonNullable<typeof settings>>(key: K) =>
    !settings || !defaults || settings[key] === defaults[key];
  const reset = <K extends keyof NonNullable<typeof settings>>(key: K) =>
    defaults && save({ [key]: defaults[key] });

  return (
    <div ref={rootRef} className="flex h-full flex-col">
      <header className="flex items-center gap-1 px-2 pt-3 pb-1">
        <IconButton label="返回" onClick={onBack}>
          <ArrowLeft size={16} />
        </IconButton>
        <h1 className="text-sm font-semibold">设置</h1>
      </header>

      <div ref={scrollRef} className="scroll-area min-h-0 flex-1 overflow-y-auto">
        <div ref={contentRef} className="pb-3">
          {settings && (
            <>
              <Section title="运行">
                <AutostartSetting />
                <SettingRow
                  title="运行模式"
                  help={MODES_HELP}
                  isDefault={isDefault("windowPolicy")}
                  onReset={() => reset("windowPolicy")}
                >
                  <Select
                    label="运行模式"
                    value={settings.windowPolicy}
                    options={MODES}
                    onChange={(windowPolicy) => save({ windowPolicy })}
                  />
                </SettingRow>
                {settings.windowPolicy === "smart" && (
                  <SettingRow
                    title="释放等待时间"
                    help={`窗口隐藏多久后释放界面，${MIN_SECONDS}–${MAX_SECONDS} 秒`}
                    isDefault={isDefault("smartReleaseSeconds")}
                    onReset={() => reset("smartReleaseSeconds")}
                  >
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
                  </SettingRow>
                )}
              </Section>

              <Section title="外观">
                <SettingRow
                  title="主题"
                  help="界面的深浅色，跟随系统时与 Windows 的“应用模式”一致"
                  isDefault={isDefault("theme")}
                  onReset={() => reset("theme")}
                >
                  <Select
                    label="主题"
                    value={settings.theme}
                    options={THEMES}
                    onChange={(theme) => save({ theme })}
                  />
                </SettingRow>
                <SettingRow
                  title="强调色"
                  help="滑块、开关等的颜色。第一个（A）跟随 Windows 强调色"
                  isDefault={isDefault("accent")}
                  onReset={() => reset("accent")}
                  below={
                    <AccentPicker
                      value={settings.accent}
                      onChange={(accent) => save({ accent })}
                    />
                  }
                />
                <WidthSetting
                  value={settings.windowWidth}
                  defaultValue={defaults?.windowWidth}
                  onCommit={(windowWidth) => save({ windowWidth })}
                />
                <SettingRow
                  title="系统音量置底"
                  help="系统音量放在应用列表下方，更靠近任务栏"
                  isDefault={isDefault("masterAtBottom")}
                  onReset={() => reset("masterAtBottom")}
                >
                  <Switch
                    label="系统音量置底"
                    checked={settings.masterAtBottom}
                    onChange={(masterAtBottom) => save({ masterAtBottom })}
                  />
                </SettingRow>
                <SettingRow
                  title="应用倒序排列"
                  help="正在播放的应用排在列表底部，更靠近任务栏"
                  isDefault={isDefault("appsReversed")}
                  onReset={() => reset("appsReversed")}
                >
                  <Switch
                    label="应用倒序排列"
                    checked={settings.appsReversed}
                    onChange={(appsReversed) => save({ appsReversed })}
                  />
                </SettingRow>
              </Section>

              <Section title="其他">
                <SettingRow
                  title="音量提示音"
                  help="调节系统音量后播放提示音，声音大小即当前音量"
                  isDefault={isDefault("volumeFeedback")}
                  onReset={() => reset("volumeFeedback")}
                >
                  <Switch
                    label="音量提示音"
                    checked={settings.volumeFeedback}
                    onChange={(volumeFeedback) => save({ volumeFeedback })}
                  />
                </SettingRow>
                <SettingRow
                  title="动画帧率"
                  help={
                    settings.animationFps === null
                      ? `当前跟随显示器刷新率${refreshRate ? `（${refreshRate} Hz）` : ""}；可填 ${MIN_FPS}–${MAX_FPS} 帧`
                      : `${MIN_FPS}–${MAX_FPS} 帧，清空则跟随显示器刷新率`
                  }
                  isDefault={isDefault("animationFps")}
                  onReset={() => reset("animationFps")}
                >
                  <span className="flex items-center gap-1.5">
                    <NumberInput
                      label="动画帧率（帧 / 秒）"
                      value={settings.animationFps}
                      min={MIN_FPS}
                      max={MAX_FPS}
                      step={1}
                      optional
                      placeholder={refreshRate ? String(refreshRate) : "自动"}
                      onCommit={(animationFps) => save({ animationFps })}
                    />
                    帧
                  </span>
                </SettingRow>
                <SettingRow
                  title="硬件加速"
                  help="使用 GPU 渲染界面。关闭时窗口显示期间少占约 70 MB 内存，界面效果不变。重启程序后生效"
                  isDefault={isDefault("hardwareAcceleration")}
                  onReset={() => {
                    setRestartNeeded(true);
                    reset("hardwareAcceleration");
                  }}
                >
                  {restartNeeded && (
                    <span className="text-xs text-muted-foreground">重启后生效</span>
                  )}
                  <Switch
                    label="硬件加速"
                    checked={settings.hardwareAcceleration}
                    onChange={(hardwareAcceleration) => {
                      setRestartNeeded(true);
                      save({ hardwareAcceleration });
                    }}
                  />
                </SettingRow>
                <SettingRow
                  title="调试工具"
                  help="在主界面显示“添加占位应用”按钮，用于测试多应用布局"
                  isDefault={isDefault("debugTools")}
                  onReset={() => reset("debugTools")}
                >
                  <Switch
                    label="调试工具"
                    checked={settings.debugTools}
                    onChange={(debugTools) => save({ debugTools })}
                  />
                </SettingRow>
              </Section>

              {error && <p className="mt-2 px-4 text-xs text-destructive">{error}</p>}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
