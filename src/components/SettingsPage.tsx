import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  commands,
  type SavedApp,
  type ThemeMode,
  type TrayStyle,
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
import { useAudioStore } from "@/stores/audio";
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

const TRAY_STYLES: SelectOption<TrayStyle>[] = [
  { value: "speaker", label: "系统样式" },
  { value: "headphones", label: "耳机" },
  { value: "note", label: "音符" },
  { value: "number", label: "音量数字" },
];

/** 托盘图标颜色：`null` 跟随任务栏（深色任务栏为白色，浅色为黑色）。偏亮的颜色在深色任务栏上更清楚。 */
const TRAY_COLORS: { value: string | null; label: string }[] = [
  { value: null, label: "跟随任务栏" },
  { value: "#FFFFFF", label: "白" },
  { value: "#000000", label: "黑" },
  { value: "#60CDFF", label: "蓝" },
  { value: "#6CCB5F", label: "绿" },
  { value: "#FFD400", label: "黄" },
  { value: "#FF8C00", label: "橙" },
  { value: "#FF4F5E", label: "红" },
  { value: "#C8A2FF", label: "紫" },
];

/** 与 `tray/glyph.rs` 中的字符一致。 */
function trayGlyph(style: TrayStyle, volume: number, muted: boolean): string {
  if (muted && (style === "speaker" || style === "number")) return "\uE74F";
  switch (style) {
    case "headphones":
      return "\uE7F6";
    case "note":
      return "\uEC4F";
    case "number":
      return String(Math.round(volume * 100));
    default: {
      const percent = Math.round(volume * 100);
      if (percent === 0) return "\uE992";
      return percent <= 33 ? "\uE993" : percent <= 66 ? "\uE994" : "\uE995";
    }
  }
}

/** 在深色、浅色两种任务栏背景上预览托盘图标。 */
function TrayPreview({ style, color }: { style: TrayStyle; color: string | null }) {
  const master = useAudioStore((s) => s.snapshot?.device?.master);
  const volume = master?.volume ?? 0.5;
  const muted = master?.muted ?? false;
  const text = trayGlyph(style, volume, muted);
  const dim = muted && (style === "headphones" || style === "note");
  return (
    <div aria-hidden className="flex gap-1.5">
      {[
        { bg: "#1C1C1C", auto: "#FFFFFF" },
        { bg: "#EEEEEE", auto: "#000000" },
      ].map((bar) => (
        <span
          key={bar.bg}
          style={{ backgroundColor: bar.bg, color: color ?? bar.auto }}
          className="flex h-7 w-9 items-center justify-center rounded-md border border-border"
        >
          <span
            style={{
              fontFamily:
                style === "number" && !muted
                  ? '"Segoe UI", sans-serif'
                  : '"Segoe Fluent Icons", "Segoe MDL2 Assets"',
              opacity: dim ? 0.35 : 1,
            }}
            className={style === "number" && !muted ? "text-[11px] font-semibold" : "text-base"}
          >
            {text}
          </span>
        </span>
      ))}
    </div>
  );
}

/** 托盘图标颜色色块，第一个（A）跟随任务栏。 */
function TrayColorPicker({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (color: string | null) => void;
}) {
  return (
    <div role="radiogroup" aria-label="托盘图标颜色" className="mt-1.5 flex flex-wrap gap-2">
      {TRAY_COLORS.map((swatch) => {
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
            style={{
              background:
                swatch.value ?? "linear-gradient(135deg, #FFFFFF 0 50%, #1C1C1C 50% 100%)",
            }}
            className={cn(
              "relative size-6 rounded-full border border-border ring-offset-2 ring-offset-card transition-shadow",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              selected ? "ring-2 ring-foreground/70" : "hover:ring-2 hover:ring-foreground/25",
            )}
          />
        );
      })}
    </div>
  );
}

/** 与后端 `config::WHEEL_STEP_RANGE` 保持一致：2–10 的偶数。 */
type WheelStep = "2" | "4" | "6" | "8" | "10";
const WHEEL_STEPS: SelectOption<WheelStep>[] = (["2", "4", "6", "8", "10"] as const).map(
  (value) => ({ value, label: `${value}%` }),
);

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
 * 开机自启。状态直接读写系统中的注册，不经过设置文件。首次运行时在主界面询问，没有默认值。
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
      below={error && <p className="text-xs text-destructive">{error}</p>}
    >
      <Switch label="开机自启" checked={enabled} onChange={(next) => void change(next)} />
    </SettingRow>
  );
}

/** 已置顶或已隐藏的应用列表，每项右侧一个取消按钮。 */
function SavedAppList({
  apps,
  actionLabel,
  onAction,
}: {
  apps: SavedApp[];
  actionLabel: string;
  onAction: (appId: string) => void;
}) {
  return (
    <ul className="flex flex-col">
      {apps.map((app) => (
        <li key={app.appId} className="flex items-center justify-between gap-2 py-0.5">
          <span className="truncate" title={app.appId}>
            {app.name}
          </span>
          <button
            type="button"
            onClick={() => onAction(app.appId)}
            className="shrink-0 rounded-md px-2 py-0.5 text-xs text-primary hover:bg-accent"
          >
            {actionLabel}
          </button>
        </li>
      ))}
    </ul>
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
  const unpinApp = useSettingsStore((s) => s.unpinApp);
  const unhideApp = useSettingsStore((s) => s.unhideApp);
  const renameApp = useSettingsStore((s) => s.renameApp);
  const deleteScene = useSettingsStore((s) => s.deleteScene);
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
  /** 置顶 / 隐藏列表中，重命名过的应用显示新名称。 */
  const withAliases = (apps: SavedApp[]) =>
    apps.map((app) => ({
      ...app,
      name: settings?.appAliases.find((a) => a.appId === app.appId)?.alias ?? app.name,
    }));

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
                <SettingRow
                  title="托盘图标"
                  help="任务栏通知区域中的图标样式和颜色。颜色的第一项跟随任务栏深浅色：深色任务栏为白色，浅色为黑色"
                  isDefault={isDefault("trayStyle") && isDefault("trayColor")}
                  onReset={() =>
                    defaults &&
                    save({ trayStyle: defaults.trayStyle, trayColor: defaults.trayColor })
                  }
                  below={
                    <>
                      <TrayColorPicker
                        value={settings.trayColor}
                        onChange={(trayColor) => save({ trayColor })}
                      />
                      <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                        <TrayPreview style={settings.trayStyle} color={settings.trayColor} />
                        预览：深色 / 浅色任务栏
                      </div>
                    </>
                  }
                >
                  <Select
                    label="托盘图标样式"
                    value={settings.trayStyle}
                    options={TRAY_STYLES}
                    onChange={(trayStyle) => save({ trayStyle })}
                  />
                </SettingRow>
              </Section>

              {/* 只显示有内容的一项；都没有时整张卡片不显示（置顶和隐藏在主界面右键菜单中操作）。 */}
              {(settings.pinnedApps.length > 0 ||
                settings.hiddenApps.length > 0 ||
                settings.appAliases.length > 0 ||
                settings.scenes.length > 0) && (
                <Section title="应用">
                  {settings.pinnedApps.length > 0 && (
                    <SettingRow
                      title="置顶的应用"
                      help="在主界面右键应用选择“置顶”。置顶的应用排在最前，可拖动左侧手柄调整顺序"
                      below={
                        <SavedAppList
                          apps={withAliases(settings.pinnedApps)}
                          actionLabel="取消置顶"
                          onAction={(appId) => void unpinApp(appId)}
                        />
                      }
                    />
                  )}
                  {settings.hiddenApps.length > 0 && (
                    <SettingRow
                      title="隐藏的应用"
                      help="在主界面右键应用选择“隐藏”。隐藏的应用不在列表中显示，音量不受影响"
                      below={
                        <SavedAppList
                          apps={withAliases(settings.hiddenApps)}
                          actionLabel="取消隐藏"
                          onAction={(appId) => void unhideApp(appId)}
                        />
                      }
                    />
                  )}
                  {settings.appAliases.length > 0 && (
                    <SettingRow
                      title="重命名的应用"
                      help="在主界面右键应用选择“重命名”。只改变本程序中显示的名称"
                      below={
                        <SavedAppList
                          apps={settings.appAliases.map((a) => ({
                            appId: a.appId,
                            name: `${a.alias}（${a.name}）`,
                          }))}
                          actionLabel="恢复原名"
                          onAction={(appId) => {
                            const app = settings.appAliases.find((a) => a.appId === appId);
                            if (app) void renameApp(app, null);
                          }}
                        />
                      }
                    />
                  )}
                  {settings.scenes.length > 0 && (
                    <SettingRow
                      title="音量场景"
                      help="在主界面点击标题栏的书签按钮，把当前各应用的音量保存为场景；点击场景即可切换。右键场景可覆盖、重命名"
                      below={
                        <SavedAppList
                          apps={settings.scenes.map((scene) => ({
                            appId: scene.id,
                            name: `${scene.name}（${scene.apps.length} 个应用）`,
                          }))}
                          actionLabel="删除"
                          onAction={(sceneId) => void deleteScene(sceneId)}
                        />
                      }
                    />
                  )}
                </Section>
              )}

              <Section title="滚轮">
                <SettingRow
                  title="任务栏滚轮"
                  help="在任务栏任意位置滚动滚轮调节系统音量。关闭时只在托盘图标上响应滚轮"
                  isDefault={isDefault("taskbarWheel")}
                  onReset={() => reset("taskbarWheel")}
                >
                  <Switch
                    label="任务栏滚轮"
                    checked={settings.taskbarWheel}
                    onChange={(taskbarWheel) => save({ taskbarWheel })}
                  />
                </SettingRow>
                <SettingRow
                  title="每格音量"
                  help="在托盘图标或任务栏上滚动一格调节的音量"
                  isDefault={isDefault("wheelStep")}
                  onReset={() => reset("wheelStep")}
                >
                  <Select
                    label="每格音量"
                    value={String(settings.wheelStep) as WheelStep}
                    options={WHEEL_STEPS}
                    onChange={(step) => save({ wheelStep: Number(step) })}
                  />
                </SettingRow>
                <SettingRow
                  title="滚轮提示音"
                  help="在托盘图标或任务栏上滚动停止后播放提示音，声音大小即当前音量"
                  isDefault={isDefault("wheelFeedback")}
                  onReset={() => reset("wheelFeedback")}
                >
                  <Switch
                    label="滚轮提示音"
                    checked={settings.wheelFeedback}
                    onChange={(wheelFeedback) => save({ wheelFeedback })}
                  />
                </SettingRow>
                <SettingRow
                  title="显示系统音量浮层"
                  help="滚动时显示 Windows 自带的音量浮层（与按键盘音量键时相同）。与音量键一样，向上滚动会取消静音"
                  isDefault={isDefault("wheelOsd")}
                  onReset={() => reset("wheelOsd")}
                >
                  <Switch
                    label="显示系统音量浮层"
                    checked={settings.wheelOsd}
                    onChange={(wheelOsd) => save({ wheelOsd })}
                  />
                </SettingRow>
              </Section>

              <Section title="其他">
                <SettingRow
                  title="音量提示音"
                  help="在窗口中调节系统音量后播放提示音，声音大小即当前音量"
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
