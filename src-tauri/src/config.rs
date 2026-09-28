//! 用户设置：保存在应用配置目录下的 `settings.json`。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, Runtime};

use crate::error::{AppError, AppResult, ErrorCode};

const FILE_NAME: &str = "settings.json";

/// 窗口隐藏时如何处理 WebView。实测数据见 docs/architecture.md“决策记录”。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WindowPolicy {
    /// 只隐藏窗口：打开约 20 ms，常驻内存约 195 MB。
    #[default]
    Keep,
    /// 销毁窗口与 WebView：打开约 0.55 秒，常驻内存约 18 MB。
    Destroy,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub window_policy: WindowPolicy,
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

        // 仅用于对比测量，不写回文件。
        match std::env::var("VOLUME_MIXER_WINDOW").as_deref() {
            Ok("keep") => settings.window_policy = WindowPolicy::Keep,
            Ok("destroy") => settings.window_policy = WindowPolicy::Destroy,
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
    fn 默认保留webview() {
        assert_eq!(Settings::default().window_policy, WindowPolicy::Keep);
    }

    #[test]
    fn 缺少字段时使用默认值() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn 序列化为驼峰命名() {
        let settings = Settings {
            window_policy: WindowPolicy::Destroy,
        };
        assert_eq!(
            serde_json::to_string(&settings).unwrap(),
            r#"{"windowPolicy":"destroy"}"#
        );
    }
}
