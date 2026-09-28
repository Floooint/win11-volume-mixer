//! 用户设置：保存在应用配置目录下的 `settings.json`。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, Runtime};

use crate::animation::FPS_RANGE;
use crate::error::{AppError, AppResult, ErrorCode};

const FILE_NAME: &str = "settings.json";

/// “智能”模式释放界面前等待的秒数范围。
pub const SMART_SECONDS_RANGE: std::ops::RangeInclusive<u32> = 10..=600;
const DEFAULT_SMART_SECONDS: u32 = 300;

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            window_policy: WindowPolicy::default(),
            smart_release_seconds: DEFAULT_SMART_SECONDS,
            volume_feedback: true,
            debug_tools: false,
            animation_fps: None,
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
        self
    }
}

/// 设置的内存副本，Tauri 全局状态。
pub struct Config {
    path: Option<PathBuf>,
    settings: Mutex<Settings>,
}

impl Config {
    /// 读取设置；文件不存在或损坏时使用默认值，不阻止程序启动。
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        let path = app
            .path()
            .app_config_dir()
            .ok()
            .map(|dir| dir.join(FILE_NAME));
        let mut settings: Settings = path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|text| match serde_json::from_str(&text) {
                Ok(settings) => Some(settings),
                Err(e) => {
                    eprintln!("[config] 设置文件无法解析，使用默认值：{e}");
                    None
                }
            })
            .unwrap_or_default();
        settings = settings.normalized();

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
        }
    }

    pub fn get(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 保存设置，返回修改前的值。
    pub fn set(&self, settings: Settings) -> AppResult<Settings> {
        let settings = settings.normalized();
        let path = self.path.as_ref().ok_or_else(save_failed)?;
        let text = serde_json::to_string_pretty(&settings).map_err(|_| save_failed())?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| log_io(&e))?;
        }
        fs::write(path, text).map_err(|e| log_io(&e))?;

        let mut current = self.settings.lock().map_err(|_| save_failed())?;
        Ok(std::mem::replace(&mut *current, settings))
    }
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
    fn 默认智能模式且等待300秒() {
        let settings = Settings::default();
        assert_eq!(settings.window_policy, WindowPolicy::Smart);
        assert_eq!(settings.smart_release_seconds, 300);
        assert!(settings.volume_feedback, "默认开启提示音");
        assert!(!settings.debug_tools, "默认关闭调试工具");
        assert_eq!(settings.animation_fps, None, "默认跟随显示器刷新率");
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
        };
        assert_eq!(
            serde_json::to_string(&settings).unwrap(),
            r#"{"windowPolicy":"smart","smartReleaseSeconds":60,"volumeFeedback":false,"debugTools":true,"animationFps":120}"#
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
