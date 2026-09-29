//! 开机自启：登录后启动到托盘（本程序启动时本来就不弹出窗口）。
//! 状态以系统中的注册（注册表 Run 项）为准，不另存到设置文件，避免两者不一致。

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;

use crate::config::Config;
use crate::error::{AppError, AppResult, ErrorCode};

pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> AppResult<()> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| {
        eprintln!("[autostart] 修改开机自启失败：{e}");
        AppError::new(ErrorCode::ConfigFailure, "开机自启设置失败")
    })
}

/// 首次运行时默认开启。开发版不自动开启，否则会把调试版 exe 注册为开机启动。
pub fn enable_on_first_run<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(debug_assertions) || !app.state::<Config>().is_first_run() {
        return;
    }
    let _ = set_enabled(app, true);
}
