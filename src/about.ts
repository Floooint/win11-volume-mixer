/**
 * 设置页“关于”中显示的内容。在这里填写，改完重新构建即可。
 *
 * - 软件名称和版本号不在这里：名称取自 src-tauri/tauri.conf.json 的 `productName`，
 *   版本号取自同一文件的 `version`（发布新版本时改这里，同时改 package.json 和 src-tauri/Cargo.toml）。
 * - 链接留空（""）时，正式版不显示这一行；开发模式下显示“未填写”，方便检查。
 * - 赞助二维码图片放在 src/assets/sponsor/ 下，文件名为 alipay.png 和 wechat.png（也可用 .jpg / .webp）。
 *   没有图片时显示占位框；两张都没有且 `sponsor.enabled` 为 false 时不显示“赞助”。
 */
export const ABOUT = {
  /** 一句话介绍，显示在名称下方。 */
  tagline: "面向 Windows 11 的轻量级托盘音量控制工具",

  author: {
    name: "Floooint",
    bilibili: "https://space.bilibili.com/321201613",
  },

  /** 项目主页，如 "https://github.com/<用户名>/<仓库名>"。 */
  homepage: "https://github.com/Floooint/win11-volume-mixer",
  /** 反馈问题的地址，如 "https://github.com/<用户名>/<仓库名>/issues"。 */
  issues: "https://github.com/Floooint/win11-volume-mixer/issues",
  /** 下载 / 发布页（可选），如 "https://github.com/<用户名>/<仓库名>/releases"。 */
  releases: "https://github.com/Floooint/win11-volume-mixer/releases",

  /** 本项目代码的许可证。 */
  license: "MIT",
  /** 版权行。 */
  copyright: "",

  sponsor: {
    /** 是否显示“赞助”。 */
    enabled: true,
    /** 赞助文案，可用 \n 换行。 */
    text: "如果你感觉软件不错，可以请我喝杯奶茶！",
  },
};

/** 使用的主要开源项目。`license` 为其许可证，`url` 为主页。 */
export const CREDITS: { name: string; note: string; license: string; url: string }[] = [
  { name: "Tauri", note: "应用框架", license: "MIT / Apache-2.0", url: "https://tauri.app" },
  { name: "React", note: "界面", license: "MIT", url: "https://react.dev" },
  { name: "TypeScript", note: "前端语言", license: "Apache-2.0", url: "https://www.typescriptlang.org" },
  { name: "Tailwind CSS", note: "样式", license: "MIT", url: "https://tailwindcss.com" },
  { name: "Motion", note: "动画", license: "MIT", url: "https://motion.dev" },
  { name: "Zustand", note: "状态管理", license: "MIT", url: "https://github.com/pmndrs/zustand" },
  { name: "Radix UI", note: "基础组件", license: "MIT", url: "https://www.radix-ui.com" },
  {
    name: "Animate UI",
    note: "动画图标与组件",
    license: "MIT + Commons Clause",
    url: "https://animate-ui.com",
  },
  { name: "shadcn/ui", note: "组件样式", license: "MIT", url: "https://ui.shadcn.com" },
  { name: "Lucide", note: "图标", license: "ISC", url: "https://lucide.dev" },
  {
    name: "windows-rs",
    note: "调用 Windows Core Audio",
    license: "MIT / Apache-2.0",
    url: "https://github.com/microsoft/windows-rs",
  },
  {
    name: "tauri-specta",
    note: "前后端类型生成",
    license: "MIT",
    url: "https://github.com/specta-rs/tauri-specta",
  },
];

/** 参考过做法的项目。不需要时清空数组即可。 */
export const REFERENCES: { name: string; note: string; url: string }[] = [
  {
    name: "EarTrumpet",
    note: "托盘滚轮用 Raw Input 的做法",
    url: "https://github.com/File-New-Project/EarTrumpet",
  },
  {
    name: "Windhawk · taskbar-volume-control",
    note: "显示系统音量浮层的做法",
    url: "https://windhawk.net",
  },
];
