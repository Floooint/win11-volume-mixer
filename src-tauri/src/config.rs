//! 用户设置：保存在程序所在目录的 `settings.json`，便于备份和随程序一起移动。
//! 程序目录不可写（如安装到 Program Files）时，改存到应用配置目录（`%APPDATA%\<identifier>`）。
//! 旧版本的设置保存在应用配置目录，首次使用新位置时复制过来（不删除旧文件）。
//!
//! 写入时先写临时文件再替换，中途崩溃不会留下半截文件。读取时某一项无法解析（如降级后
//! 遇到新版本的取值）只让该项使用默认值；整个文件都无法解析时先备份为 `settings.json.bad`。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, Runtime};

use crate::animation::FPS_RANGE;
use crate::error::{AppError, AppResult, ErrorCode};

const FILE_NAME: &str = "settings.json";
/// 指定设置文件的完整路径，仅用于测试（如模拟首次运行），不影响正常使用。
const PATH_ENV: &str = "VOLUME_MIXER_SETTINGS";

/// “智能”模式释放界面前等待的秒数范围。
pub const SMART_SECONDS_RANGE: std::ops::RangeInclusive<u32> = 10..=600;
const DEFAULT_SMART_SECONDS: u32 = 300;

/// 托盘 / 任务栏滚轮每格调节的百分点。
pub const WHEEL_STEP_RANGE: std::ops::RangeInclusive<u32> = 1..=10;
const DEFAULT_WHEEL_STEP: u32 = 2;

/// 窗口宽度范围（逻辑像素），须与 `tauri.conf.json` 的 `minWidth` / `maxWidth` 一致。
pub const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 280..=500;
const DEFAULT_WIDTH: u32 = 340;

/// 窗口隐藏后的运行模式。实测数据见 docs/architecture.md“决策记录”。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WindowPolicy {
    /// 常驻：只隐藏窗口。打开约 20 ms，常驻内存约 195 MB。
    #[serde(alias = "keep")]
    Resident,
    /// 静默：隐藏即释放界面。打开约 0.55 秒，常驻内存约 3 MB。
    #[serde(alias = "destroy")]
    Silent,
    /// 智能：隐藏后保留界面，连续 `smart_release_seconds` 秒未打开才释放。
    #[default]
    Smart,
}

/// 界面深浅色。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    /// 跟随 Windows 的“应用模式”。
    #[default]
    System,
    Light,
    Dark,
}

/// 场景中一个应用的音量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SceneApp {
    pub app_id: String,
    pub name: String,
    /// 音量（0–1）。
    pub volume: f64,
    pub muted: bool,
}

/// 音量场景：一组应用的音量组合，一键切换（如“游戏”“会议”“音乐”）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    /// 场景标识，创建时由前端生成，不随改名变化。
    pub id: String,
    pub name: String,
    pub apps: Vec<SceneApp>,
}

/// 托盘图标样式，见 `tray/glyph.rs`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TrayStyle {
    /// 与 Windows 自带的音量图标一致，随音量分档变化。
    #[default]
    Speaker,
    Headphones,
    /// 音符。
    Note,
    /// 显示音量数字。
    Number,
}

/// 置顶或隐藏的应用。记下名称，应用没在运行时也能在设置页中显示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedApp {
    pub app_id: String,
    pub name: String,
}

/// 重命名的应用。`name` 为原来的应用名，应用没在运行时设置页也能显示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppAlias {
    pub app_id: String,
    pub name: String,
    /// 用户起的名称，界面中优先显示。
    pub alias: String,
}

/// 重命名的最大长度（字符）。
pub const MAX_ALIAS_CHARS: usize = 40;

/// 分组中的一个应用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub app_id: String,
    pub name: String,
    /// 该应用在组音量 100% 时的音量（0–1）。实际音量 = 此值 × 组音量。
    pub full_volume: f64,
}

/// 应用分组：组内应用在主界面合并为一行，由组音量按比例统一调节。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppGroup {
    /// 分组标识，创建时由前端生成，不随改名变化。
    pub id: String,
    pub name: String,
    pub apps: Vec<GroupMember>,
    /// 组音量（0–1）。组内应用的音量 = 该应用在组音量 100% 时的音量 × 组音量。
    pub volume: f64,
    /// 在主界面中展开显示组内应用。
    #[serde(default)]
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub window_policy: WindowPolicy,
    /// 智能模式下，窗口隐藏多少秒后释放界面。
    pub smart_release_seconds: u32,
    /// 调节系统音量后播放提示音。
    pub volume_feedback: bool,
    /// 在主界面显示调试工具（添加占位应用）。
    pub debug_tools: bool,
    /// 窗口滑入 / 滑出动画的帧率（帧 / 秒）。`None` 表示跟随显示器刷新率。
    pub animation_fps: Option<u32>,
    /// 窗口宽度（逻辑像素）。
    pub window_width: u32,
    /// 系统音量放在应用列表下方（靠近任务栏），默认开启。
    pub master_at_bottom: bool,
    /// 应用列表倒序：活跃应用排在底部，更靠近任务栏。默认开启。
    pub apps_reversed: bool,
    /// 标题栏显示“固定窗口”按钮。
    pub show_pin_button: bool,
    /// 标题栏显示“新建分组”按钮。
    pub show_group_button: bool,
    /// 标题栏显示“保存为场景”按钮。
    pub show_scene_button: bool,
    /// 界面使用 GPU 渲染，默认开启。关闭时窗口显示期间少占约 70 MB 内存（实测见
    /// docs/architecture.md），界面简单，软件渲染也足够流畅。重启程序后生效。
    pub hardware_acceleration: bool,
    pub theme: ThemeMode,
    /// 强调色 `#RRGGBB`；`None` 表示跟随 Windows 强调色。
    pub accent: Option<String>,
    /// 置顶的应用，按显示顺序排列（可拖拽调整）。
    pub pinned_apps: Vec<SavedApp>,
    /// 隐藏的应用。
    pub hidden_apps: Vec<SavedApp>,
    /// 应用分组，按显示顺序排列。一个应用只属于一个分组。
    pub groups: Vec<AppGroup>,
    /// 重命名的应用。
    pub app_aliases: Vec<AppAlias>,
    /// 音量场景，按显示顺序排列。
    pub scenes: Vec<Scene>,
    pub tray_style: TrayStyle,
    /// 在任务栏任意位置滚动滚轮调节系统音量（默认只在托盘图标上）。
    pub taskbar_wheel: bool,
    /// 托盘 / 任务栏滚轮每格调节的百分点（1–10）。
    pub wheel_step: u32,
    /// 托盘 / 任务栏滚轮停止后播放提示音。
    pub wheel_feedback: bool,
    /// 托盘 / 任务栏滚轮调节时显示 Windows 自带的音量浮层。
    pub wheel_osd: bool,
    /// 托盘图标颜色 `#RRGGBB`；`None` 表示跟随任务栏深浅色（深色任务栏为白色，浅色为黑色）。
    pub tray_color: Option<String>,
    /// 还没询问过是否开机自启：首次运行时为 `true`，主界面据此弹出询问，回答后清除。
    pub autostart_prompt: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            window_policy: WindowPolicy::default(),
            smart_release_seconds: DEFAULT_SMART_SECONDS,
            volume_feedback: true,
            debug_tools: false,
            animation_fps: None,
            window_width: DEFAULT_WIDTH,
            master_at_bottom: true,
            apps_reversed: true,
            show_pin_button: true,
            show_group_button: true,
            show_scene_button: true,
            hardware_acceleration: true,
            theme: ThemeMode::System,
            accent: None,
            pinned_apps: Vec::new(),
            hidden_apps: Vec::new(),
            groups: Vec::new(),
            app_aliases: Vec::new(),
            scenes: Vec::new(),
            tray_style: TrayStyle::default(),
            taskbar_wheel: true,
            wheel_step: DEFAULT_WHEEL_STEP,
            wheel_feedback: true,
            wheel_osd: true,
            tray_color: None,
            autostart_prompt: false,
        }
    }
}

impl Settings {
    /// 把超出范围的值修正到合法范围，防止手动编辑设置文件写入异常值。
    fn normalized(mut self) -> Self {
        self.smart_release_seconds = self
            .smart_release_seconds
            .clamp(*SMART_SECONDS_RANGE.start(), *SMART_SECONDS_RANGE.end());
        self.animation_fps = self
            .animation_fps
            .map(|fps| fps.clamp(*FPS_RANGE.start(), *FPS_RANGE.end()));
        self.accent = self.accent.filter(|color| is_hex_color(color));
        self.tray_color = self.tray_color.filter(|color| is_hex_color(color));
        self.pinned_apps = saved_apps(self.pinned_apps);
        self.hidden_apps = saved_apps(self.hidden_apps);
        self.groups = normalized_groups(self.groups);
        self.app_aliases = normalized_aliases(self.app_aliases);
        self.scenes = normalized_scenes(self.scenes);
        self.window_width = self
            .window_width
            .clamp(*WIDTH_RANGE.start(), *WIDTH_RANGE.end());
        self.wheel_step = self
            .wheel_step
            .clamp(*WHEEL_STEP_RANGE.start(), *WHEEL_STEP_RANGE.end());
        self
    }
}

/// 去重，并去掉以 PID 标识的应用：进程重启后 PID 会变，记住也没有意义。
fn saved_apps(apps: Vec<SavedApp>) -> Vec<SavedApp> {
    let mut seen = std::collections::HashSet::new();
    apps.into_iter()
        .filter(|app| !app.app_id.starts_with("pid:") && seen.insert(app.app_id.clone()))
        .collect()
}

/// 去重、去掉以 PID 标识的应用；名称去掉首尾空白并限制长度，为空的去掉。
fn normalized_aliases(aliases: Vec<AppAlias>) -> Vec<AppAlias> {
    let mut seen = std::collections::HashSet::new();
    aliases
        .into_iter()
        .filter_map(|mut a| {
            a.alias = a.alias.trim().chars().take(MAX_ALIAS_CHARS).collect();
            let keep = !a.alias.is_empty()
                && !a.app_id.starts_with("pid:")
                && seen.insert(a.app_id.clone());
            keep.then_some(a)
        })
        .collect()
}

/// 去掉重复的场景和场景中重复的应用，以及以 PID 标识的应用；音量限制在 0–1。
fn normalized_scenes(scenes: Vec<Scene>) -> Vec<Scene> {
    let mut ids = std::collections::HashSet::new();
    scenes
        .into_iter()
        .filter(|scene| ids.insert(scene.id.clone()))
        .map(|mut scene| {
            let mut apps = std::collections::HashSet::new();
            scene
                .apps
                .retain(|a| !a.app_id.starts_with("pid:") && apps.insert(a.app_id.clone()));
            for app in &mut scene.apps {
                app.volume = unit_or_full(app.volume);
            }
            scene
        })
        .collect()
}

/// 去掉重复的分组；一个应用只保留在它最先出现的分组中；组音量限制在 0–1。
fn normalized_groups(groups: Vec<AppGroup>) -> Vec<AppGroup> {
    let mut group_ids = std::collections::HashSet::new();
    let mut app_ids = std::collections::HashSet::new();
    groups
        .into_iter()
        .filter(|group| group_ids.insert(group.id.clone()))
        .map(|mut group| {
            group.apps = group
                .apps
                .into_iter()
                .filter(|app| !app.app_id.starts_with("pid:") && app_ids.insert(app.app_id.clone()))
                .map(|mut app| {
                    app.full_volume = unit_or_full(app.full_volume);
                    app
                })
                .collect();
            group.volume = unit_or_full(group.volume);
            group
        })
        .collect()
}

/// 限制在 0–1；无效值（NaN 等）按 1。
fn unit_or_full(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// 是否为 `#RRGGBB` 格式，防止设置文件中的异常值被写进 CSS。
fn is_hex_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// 设置的内存副本，Tauri 全局状态。
pub struct Config {
    path: Option<PathBuf>,
    settings: Mutex<Settings>,
    /// 启动时设置文件还不存在，即首次运行。
    first_run: bool,
}

impl Config {
    /// 读取设置；文件不存在或损坏时使用默认值，不阻止程序启动。
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        // 用环境变量指定位置时（测试），不读取、也不迁移旧位置的设置。
        let override_path = std::env::var_os(PATH_ENV).map(PathBuf::from);
        let legacy = app
            .path()
            .app_config_dir()
            .ok()
            .map(|dir| dir.join(FILE_NAME))
            .filter(|_| override_path.is_none());
        let path = override_path.or_else(|| settings_path(legacy.as_deref()));
        let first_run = path.as_ref().is_some_and(|p| !p.exists())
            && legacy.as_ref().is_none_or(|p| !p.exists());
        // 新位置还没有设置文件时，读取旧位置的，并复制到新位置。
        let source = match (&path, &legacy) {
            (Some(p), Some(old)) if !p.exists() && old.exists() => {
                if let Err(e) = fs::copy(old, p) {
                    eprintln!("[config] 无法把设置复制到 {}：{e}", p.display());
                }
                Some(old.clone())
            }
            _ => path.clone(),
        };
        eprintln!("[config] 设置文件：{:?}", path);
        let mut settings: Settings = source
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok().map(|text| (p, text)))
            .and_then(|(p, text)| {
                let parsed = parse_lenient(&text);
                if parsed.is_none() {
                    // 保留原文件，之后保存设置时不会把用户的数据悄悄覆盖掉。
                    let backup = p.with_extension("json.bad");
                    eprintln!(
                        "[config] 设置文件无法解析，使用默认值，原文件备份为 {}",
                        backup.display()
                    );
                    let _ = fs::copy(p, backup);
                }
                parsed
            })
            .unwrap_or_default();
        settings = settings.normalized();
        // 升级前的设置文件没有这一项，按默认值（已询问过）处理，不再打扰老用户。
        if first_run {
            settings.autostart_prompt = true;
        }

        // 仅用于对比测量，不写回文件。
        match std::env::var("VOLUME_MIXER_WINDOW").as_deref() {
            Ok("resident" | "keep") => settings.window_policy = WindowPolicy::Resident,
            Ok("silent" | "destroy") => settings.window_policy = WindowPolicy::Silent,
            Ok("smart") => settings.window_policy = WindowPolicy::Smart,
            _ => {}
        }

        Self {
            path,
            settings: Mutex::new(settings),
            first_run,
        }
    }

    pub fn is_first_run(&self) -> bool {
        self.first_run
    }

    pub fn get(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 只读取需要的几项，不复制整份设置（托盘图标随音量频繁刷新时使用）。
    pub fn read<T>(&self, f: impl FnOnce(&Settings) -> T) -> T {
        match self.settings.lock() {
            Ok(settings) => f(&settings),
            Err(_) => f(&Settings::default()),
        }
    }

    /// 保存设置，返回修改前的值。
    pub fn set(&self, settings: Settings) -> AppResult<Settings> {
        let settings = settings.normalized();
        let path = self.path.as_ref().ok_or_else(save_failed)?;
        let text = serde_json::to_string_pretty(&settings).map_err(|_| save_failed())?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| log_io(&e))?;
        }
        write_atomic(path, text.as_bytes()).map_err(|e| log_io(&e))?;

        let mut current = self.settings.lock().map_err(|_| save_failed())?;
        Ok(std::mem::replace(&mut *current, settings))
    }
}

/// 设置文件的位置：程序所在目录（可写时），否则为旧位置（应用配置目录）。
/// 保存时要在同一目录创建临时文件，所以即使程序目录中已有设置文件，目录不可写时也改用旧位置，
/// 并把程序目录中的设置复制过去（旧位置还没有设置时）。
fn settings_path(legacy: Option<&Path>) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let Some(dir) = exe_dir else {
        return legacy.map(Path::to_path_buf);
    };
    let local = dir.join(FILE_NAME);
    if is_writable(&dir) {
        return Some(local);
    }
    let legacy = legacy?;
    if local.exists() && !legacy.exists() {
        let copied = legacy
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::copy(&local, legacy));
        if let Err(e) = copied {
            eprintln!("[config] 无法把设置复制到 {}：{e}", legacy.display());
        }
    }
    Some(legacy.to_path_buf())
}

/// 能否在目录中创建文件：试着创建一个临时文件再删除。
fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".settings-write-test");
    let created = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if created {
        let _ = fs::remove_file(&probe);
    }
    created
}

/// 先写入同目录的临时文件并刷到磁盘，再替换原文件：中途崩溃或断电时原文件保持完整。
fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut file = fs::File::create(&tmp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    // Windows 上 `rename` 会覆盖已存在的目标文件。
    fs::rename(&tmp, path)
}

/// 解析设置文件。整体解析失败时逐项尝试：无法解析的项使用默认值，其余项保留。
/// 整个文件都不是 JSON 对象时返回 `None`。
fn parse_lenient(text: &str) -> Option<Settings> {
    if let Ok(settings) = serde_json::from_str(text) {
        return Some(settings);
    }
    let serde_json::Value::Object(fields) = serde_json::from_str(text).ok()? else {
        return None;
    };
    let serde_json::Value::Object(mut merged) = serde_json::to_value(Settings::default()).ok()?
    else {
        return None;
    };
    for (key, value) in fields {
        let mut trial = merged.clone();
        trial.insert(key.clone(), value);
        if serde_json::from_value::<Settings>(serde_json::Value::Object(trial.clone())).is_ok() {
            merged = trial;
        } else {
            eprintln!("[config] 设置项 {key} 无法解析，使用默认值");
        }
    }
    serde_json::from_value(serde_json::Value::Object(merged)).ok()
}

fn save_failed() -> AppError {
    AppError::new(ErrorCode::ConfigFailure, "设置保存失败")
}

fn log_io(e: &std::io::Error) -> AppError {
    eprintln!("[config] 写入设置失败：{e}");
    save_failed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 某一项无法解析时只让该项使用默认值() {
        let text = r#"{"windowWidth": 400, "theme": "未来的主题", "debugTools": true}"#;
        let settings = parse_lenient(text).expect("其余项应能读取");
        assert_eq!(settings.window_width, 400);
        assert!(settings.debug_tools);
        assert_eq!(settings.theme, Settings::default().theme);
    }

    #[test]
    fn 半截文件无法解析() {
        assert!(parse_lenient(r#"{"windowWidth": 4"#).is_none());
    }

    #[test]
    fn 原子写入会覆盖已有文件且不留临时文件() {
        let dir = std::env::temp_dir().join(format!("volume-mixer-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn 默认智能模式且等待300秒() {
        let settings = Settings::default();
        assert_eq!(settings.window_policy, WindowPolicy::Smart);
        assert_eq!(settings.smart_release_seconds, 300);
        assert!(settings.volume_feedback, "默认开启提示音");
        assert!(!settings.debug_tools, "默认关闭调试工具");
        assert_eq!(settings.animation_fps, None, "默认跟随显示器刷新率");
        assert_eq!(settings.window_width, 340);
        assert!(settings.master_at_bottom, "默认系统音量置底，靠近任务栏");
        assert!(settings.apps_reversed, "默认倒序，活跃应用在底部");
        assert!(settings.hardware_acceleration, "默认开启硬件加速");
        assert!(settings.taskbar_wheel, "默认在整个任务栏上响应滚轮");
        assert_eq!(settings.wheel_step, 2);
        assert!(settings.wheel_feedback && settings.wheel_osd);
    }

    #[test]
    fn 缺少字段时使用默认值() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn 序列化为驼峰命名() {
        let settings = Settings {
            window_policy: WindowPolicy::Smart,
            smart_release_seconds: 60,
            volume_feedback: false,
            debug_tools: true,
            animation_fps: Some(120),
            window_width: 400,
            master_at_bottom: true,
            apps_reversed: true,
            show_pin_button: true,
            show_group_button: false,
            show_scene_button: true,
            hardware_acceleration: true,
            theme: ThemeMode::Dark,
            accent: Some("#744DA9".into()),
            pinned_apps: Vec::new(),
            hidden_apps: Vec::new(),
            groups: Vec::new(),
            app_aliases: Vec::new(),
            scenes: Vec::new(),
            tray_style: TrayStyle::Number,
            taskbar_wheel: true,
            wheel_step: 4,
            wheel_feedback: false,
            wheel_osd: true,
            tray_color: Some("#FFFFFF".into()),
            autostart_prompt: false,
        };
        assert_eq!(
            serde_json::to_string(&settings).unwrap(),
            r##"{"windowPolicy":"smart","smartReleaseSeconds":60,"volumeFeedback":false,"debugTools":true,"animationFps":120,"windowWidth":400,"masterAtBottom":true,"appsReversed":true,"showPinButton":true,"showGroupButton":false,"showSceneButton":true,"hardwareAcceleration":true,"theme":"dark","accent":"#744DA9","pinnedApps":[],"hiddenApps":[],"groups":[],"appAliases":[],"scenes":[],"trayStyle":"number","taskbarWheel":true,"wheelStep":4,"wheelFeedback":false,"wheelOsd":true,"trayColor":"#FFFFFF","autostartPrompt":false}"##
        );
    }

    #[test]
    fn 帧率为空时序列化为null() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains(r#""animationFps":null"#), "{json}");
    }

    #[test]
    fn 兼容旧版设置值() {
        let old: Settings = serde_json::from_str(r#"{"windowPolicy":"keep"}"#).unwrap();
        assert_eq!(old.window_policy, WindowPolicy::Resident);
        let old: Settings = serde_json::from_str(r#"{"windowPolicy":"destroy"}"#).unwrap();
        assert_eq!(old.window_policy, WindowPolicy::Silent);
    }

    #[test]
    fn 等待秒数被限制在10到600之间() {
        let too_small = Settings {
            smart_release_seconds: 0,
            ..Settings::default()
        };
        assert_eq!(too_small.normalized().smart_release_seconds, 10);
        let too_large = Settings {
            smart_release_seconds: 9999,
            ..Settings::default()
        };
        assert_eq!(too_large.normalized().smart_release_seconds, 600);
    }

    #[test]
    fn 置顶和隐藏的应用去重并忽略_pid_标识() {
        let app = |id: &str| SavedApp {
            app_id: id.into(),
            name: id.into(),
        };
        let settings = Settings {
            pinned_apps: vec![app("a"), app("pid:42"), app("b"), app("a")],
            ..Settings::default()
        }
        .normalized();
        let ids: Vec<_> = settings
            .pinned_apps
            .iter()
            .map(|a| a.app_id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn 一个应用只属于一个分组且组音量被限制() {
        let app = |id: &str| GroupMember {
            app_id: id.into(),
            name: id.into(),
            full_volume: 1.5,
        };
        let group = |id: &str, apps: Vec<GroupMember>, volume: f64| AppGroup {
            id: id.into(),
            name: id.into(),
            apps,
            volume,
            expanded: false,
        };
        let settings = Settings {
            groups: vec![
                group("g1", vec![app("a"), app("b")], 2.0),
                group("g2", vec![app("b"), app("c")], f64::NAN),
                group("g1", vec![app("d")], 0.5),
            ],
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.groups.len(), 2, "重复的分组标识只保留第一个");
        let ids = |g: &AppGroup| g.apps.iter().map(|a| a.app_id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&settings.groups[0]), ["a", "b"]);
        assert_eq!(ids(&settings.groups[1]), ["c"], "b 已在第一个分组中");
        assert_eq!(settings.groups[0].volume, 1.0);
        assert_eq!(settings.groups[1].volume, 1.0, "无效的组音量按 100%");
        assert_eq!(settings.groups[0].apps[0].full_volume, 1.0);
    }

    #[test]
    fn 重命名去掉空白和空名称并限制长度() {
        let alias = |id: &str, alias: &str| AppAlias {
            app_id: id.into(),
            name: id.into(),
            alias: alias.into(),
        };
        let settings = Settings {
            app_aliases: vec![
                alias("a", "  音乐  "),
                alias("b", "   "),
                alias("pid:1", "x"),
                alias("a", "重复"),
                alias("c", &"长".repeat(100)),
            ],
            ..Settings::default()
        }
        .normalized();
        let aliases: Vec<_> = settings
            .app_aliases
            .iter()
            .map(|a| (a.app_id.as_str(), a.alias.chars().count()))
            .collect();
        assert_eq!(aliases, [("a", 2), ("c", MAX_ALIAS_CHARS)]);
        assert_eq!(settings.app_aliases[0].alias, "音乐");
    }

    #[test]
    fn 场景去重并限制音量() {
        let app = |id: &str, volume: f64| SceneApp {
            app_id: id.into(),
            name: id.into(),
            volume,
            muted: false,
        };
        let scene = |id: &str, apps: Vec<SceneApp>| Scene {
            id: id.into(),
            name: id.into(),
            apps,
        };
        let settings = Settings {
            scenes: vec![
                scene("s1", vec![app("a", 1.5), app("pid:3", 0.5), app("a", 0.2)]),
                scene("s1", vec![]),
                scene("s2", vec![app("a", -1.0), app("b", f64::NAN)]),
            ],
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.scenes.len(), 2);
        let volumes = |s: &Scene| s.apps.iter().map(|a| a.volume).collect::<Vec<_>>();
        assert_eq!(
            volumes(&settings.scenes[0]),
            [1.0],
            "重复的应用只保留第一个"
        );
        assert_eq!(volumes(&settings.scenes[1]), [0.0, 1.0]);
    }

    #[test]
    fn 格式错误的强调色被忽略() {
        let settings = |accent: &str| Settings {
            accent: Some(accent.into()),
            ..Settings::default()
        };
        assert_eq!(
            settings("#744DA9").normalized().accent.as_deref(),
            Some("#744DA9")
        );
        assert_eq!(settings("red").normalized().accent, None);
        assert_eq!(settings("#12345G").normalized().accent, None);
        assert_eq!(settings("#123;}").normalized().accent, None);
    }

    #[test]
    fn 宽度被限制在范围内() {
        let settings = |window_width| Settings {
            window_width,
            ..Settings::default()
        };
        assert_eq!(settings(100).normalized().window_width, 280);
        assert_eq!(settings(9999).normalized().window_width, 500);
        assert_eq!(settings(450).normalized().window_width, 450);
    }

    #[test]
    fn 滚轮步长被限制在1到10之间() {
        let settings = |wheel_step| Settings {
            wheel_step,
            ..Settings::default()
        };
        assert_eq!(settings(0).normalized().wheel_step, 1);
        assert_eq!(settings(5).normalized().wheel_step, 5);
        assert_eq!(settings(8).normalized().wheel_step, 8);
        assert_eq!(settings(99).normalized().wheel_step, 10);
    }

    #[test]
    fn 帧率被限制在30到240之间() {
        let settings = |animation_fps| Settings {
            animation_fps,
            ..Settings::default()
        };
        assert_eq!(settings(Some(5)).normalized().animation_fps, Some(30));
        assert_eq!(settings(Some(999)).normalized().animation_fps, Some(240));
        assert_eq!(settings(Some(144)).normalized().animation_fps, Some(144));
        assert_eq!(settings(None).normalized().animation_fps, None);
    }
}
