import { commands, type TimingMark } from "@/bindings";
import { useAudioStore } from "@/stores/audio";
import { useSettingsStore } from "@/stores/settings";

/**
 * 新建窗口打开过程的分段计时（后端见 src-tauri/src/timing.rs）。
 * 前端记下各阶段的时间点，内容就绪后交给后端，与后端的时间点合并输出。
 * 不是为打开而新建的窗口（如启动时预先创建）后端没有计时，报告会被忽略。
 */
const marks: TimingMark[] = [];

const epochNow = () => performance.timeOrigin + performance.now();

/** 记下一个时间点；`at` 为相对页面导航开始的毫秒数，省略时为现在。同名只记第一次（StrictMode 会执行两次）。 */
export function markOpen(name: string, at?: number) {
  if (marks.some((m) => m.name === name)) return;
  marks.push({ name, epochMs: at === undefined ? epochNow() : performance.timeOrigin + at });
}

/** 浏览器记录的页面加载时间点：导航开始、HTML 和脚本文件加载完成。 */
function markPageLoad() {
  markOpen("页面开始导航", 0);
  const [navigation] = performance.getEntriesByType("navigation") as PerformanceNavigationTiming[];
  if (navigation) markOpen("HTML 加载完成", navigation.responseEnd);
  const script = (performance.getEntriesByType("resource") as PerformanceResourceTiming[]).find(
    (entry) => entry.name.endsWith(".js") || entry.name.includes("/src/main.tsx"),
  );
  if (script) markOpen("脚本文件加载完成", script.responseEnd);
}

/**
 * 设置和应用列表（或加载失败）都已读取。模块加载时就开始订阅，读取完成的时间点才准确。
 */
const contentLoaded = new Promise<void>((resolve) => {
  const loaded = () => {
    const audio = useAudioStore.getState();
    const audioLoaded = audio.snapshot !== null || audio.loadError !== null;
    const settingsLoaded = useSettingsStore.getState().settings !== null;
    if (audioLoaded) markOpen("应用列表读取完成");
    if (settingsLoaded) markOpen("设置读取完成");
    return audioLoaded && settingsLoaded;
  };
  if (loaded()) return resolve();
  const done = () => {
    if (!loaded()) return;
    unsubscribeAudio();
    unsubscribeSettings();
    resolve();
  };
  const unsubscribeAudio = useAudioStore.subscribe(done);
  const unsubscribeSettings = useSettingsStore.subscribe(done);
});

const nextFrame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/** 通知后端显示窗口，等内容就绪并绘制后报告计时。 */
export async function readyAndReportOpenTiming() {
  markOpen("通知后端显示窗口");
  await commands.windowReady();
  await contentLoaded;
  // 状态更新后 React 重新渲染，下一帧绘制出列表。
  await nextFrame();
  markOpen("应用列表绘制（约）");
  markPageLoad();
  await commands.reportOpenTiming(marks);
}
