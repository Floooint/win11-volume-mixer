import { useEffect, useState } from "react";
import { type AccentColors, commands, events } from "@/bindings";

/** 可手动选择的强调色（Windows 个性化设置中的常用色）。 */
export const ACCENT_PRESETS: { value: string; label: string }[] = [
  { value: "#0063B1", label: "蓝" },
  { value: "#744DA9", label: "紫" },
  { value: "#C239B3", label: "粉" },
  { value: "#E81123", label: "红" },
  { value: "#CA5010", label: "橙" },
  { value: "#107C10", label: "绿" },
  { value: "#038387", label: "青" },
  { value: "#515C6B", label: "灰" },
];

/**
 * 由预设色推算浅色 / 深色主题下的强调色：保留色相，按 Windows 11 默认强调色的亮度
 * （#005FB8 约 0.5、#60CDFF 约 0.8）调整，保证在两种背景下都有足够对比度。
 */
function derive(color: string): AccentColors {
  return {
    light: `oklch(from ${color} 0.5 c h)`,
    dark: `oklch(from ${color} 0.8 calc(c * 0.8) h)`,
  };
}

/** 是否为自定义颜色（不是预设色）。 */
export function isCustomAccent(accent: string | null): accent is string {
  return accent !== null && !ACCENT_PRESETS.some((p) => p.value === accent);
}

/** `#RRGGBB` 的相对亮度（WCAG 定义）。 */
function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/** 强调色背景上的文字颜色：黑白中与背景对比度较高的一个。 */
function readableOn(hex: string): string {
  const l = luminance(hex);
  // 与白色的对比度 (1.05) / (l + 0.05)，与黑色的对比度 (l + 0.05) / 0.05。
  return 1.05 / (l + 0.05) >= (l + 0.05) / 0.05 ? "#FFFFFF" : "#1C1C1C";
}

/** Windows 强调色，随系统设置变化；读取失败时为 `null`（使用 CSS 中的默认值）。 */
export function useSystemAccent(): AccentColors | null {
  const [colors, setColors] = useState<AccentColors | null>(null);
  useEffect(() => {
    const unlisten = events.themeAccent.listen((e) => setColors(e.payload));
    commands.getAccentColors().then(setColors);
    return () => void unlisten.then((fn) => fn());
  }, []);
  return colors;
}

/**
 * 把强调色写入 CSS 变量。`accent` 为用户选择的颜色，`null` 表示跟随系统。
 * 预设色按主题调整亮度；自定义颜色在两种主题下都原样使用（所见即所得），
 * 强调色背景上的文字改为黑白中对比度较高的一个。
 */
export function useApplyAccent(accent: string | null, system: AccentColors | null) {
  useEffect(() => {
    const custom = isCustomAccent(accent);
    const colors = custom ? { light: accent, dark: accent } : accent ? derive(accent) : system;
    const style = document.documentElement.style;
    if (colors) {
      style.setProperty("--accent-light", colors.light);
      style.setProperty("--accent-dark", colors.dark);
    } else {
      style.removeProperty("--accent-light");
      style.removeProperty("--accent-dark");
    }
    if (custom) style.setProperty("--primary-foreground", readableOn(accent));
    else style.removeProperty("--primary-foreground");
  }, [accent, system]);
}
