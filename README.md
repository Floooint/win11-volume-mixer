<h1 align="center">更优雅的音量控制器</h1>

<p align="center">
  支持任务栏滚轮调节音量大小的 Windows 11 托盘音量控制工具
</p>

<p align="center">
  <a href="../../releases/latest">下载</a> ·
  <a href="../../issues">反馈问题</a> ·
  <a href="https://space.bilibili.com/321201613">B 站主页</a>
</p>

<p align="center">
  <img src="assets/screenshots/main.png" alt="主窗口" width="260">
  <img src="assets/screenshots/settings.png" alt="设置页" width="260">
</p>
<p align="center">
  <img src="assets/screenshots/details.png" alt="应用详情" width="540">
</p>

Win11 想单独调某个应用的音量，要点开快速设置、点小箭头、再往下翻到音量合成器。这个工具点一下托盘图标就能看到所有应用，直接调。

## 功能

- 每个应用单独调音量、静音，和系统音量合成器实时同步
- 滚轮在任意一行上调节，点百分比可以直接输入数值
- 托盘图标上滚轮调总音量，中键静音；也可以在整条任务栏上滚
- 应用可以置顶、隐藏、重命名、拖进分组一起调
- 音量场景：保存一套各应用的音量，一键切换
- 跟随系统深浅色和强调色，Mica 背景
- 后台占用低，静默模式下只有几 MB

## 安装

在 [Releases](../../releases/latest) 下载安装包（`*-setup.exe`）或便携版（`win11-volume-mixer_0.2.0_x64_portable.exe`）。

针对 Windows 11 x64。

程序没有签名，第一次运行可能弹出“Windows 已保护你的电脑”，点“更多信息”→“仍要运行”即可。

## 使用

| 操作           | 效果                       |
| -------------- | -------------------------- |
| 左键托盘图标   | 打开 / 收起窗口            |
| 托盘图标上滚轮 | 调总音量                   |
| 中键托盘图标   | 静音                       |
| 右键应用       | 置顶、分组、隐藏、重命名等 |
| 点击应用       | 显示进程名、路径等详情     |

设置保存在程序目录的 `settings.json`。目录不能写入时（比如在 Program Files 里）存到 `%APPDATA%\com.floooint.volumemixer\`。

## 已知问题

- 在系统自带的音量图标上滚动，系统也会调一次，两者叠加
- 部分游戏在前台时，任务栏滚轮不起作用（游戏拦截了滚轮）
- 窗口打开时 WebView2 大约占 200 MB 内存，收起后会释放

## 构建

需要 Node.js 24、pnpm 12、Rust stable 和 Visual Studio Build Tools（C++），详见 [agent.md](agent.md)。

```text
pnpm install
pnpm tauri dev      # 开发
pnpm tauri build    # 打包，输出在 src-tauri/target/release/
```

基于 Tauri 2 + React，后端用 Rust 通过 windows-rs 调用 Core Audio。

## 致谢

[Tauri](https://tauri.app)、[Animate UI](https://animate-ui.com)、[Radix UI](https://www.radix-ui.com)、[windows-rs](https://github.com/microsoft/windows-rs)，以及参考了 [EarTrumpet](https://github.com/File-New-Project/EarTrumpet) 和 [Windhawk](https://windhawk.net) 的实现思路。

## 许可证

[MIT](LICENSE)。`src/components/animate-ui/` 和 `src/hooks/use-is-in-view.tsx` 来自 Animate UI，适用 [MIT + Commons Clause](src/components/animate-ui/LICENSE.md)，不能单独出售或再分发。
