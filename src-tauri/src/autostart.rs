//! 开机自启：登录后启动到托盘（本程序启动时本来就不弹出窗口）。
//! 状态以系统中的注册（注册表 Run 项）为准，不另存到设置文件，避免两者不一致。
//! 首次运行时不自动开启，而是打开窗口询问（见 `Settings::autostart_prompt`）。

use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

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
