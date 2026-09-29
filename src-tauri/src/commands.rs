//! Tauri 命令。只负责参数校验和转发，所有音频操作在音频线程上执行。

use tauri::State;

use crate::audio::{AudioService, AudioSnapshot, Command};
use crate::config::{Config, Settings};
use crate::error::AppResult;
use crate::feedback::Feedback;

/// 获取完整状态。窗口创建后调用一次，之后依靠事件增量更新。
#[tauri::command]
#[specta::specta]
pub async fn get_snapshot(audio: State<'_, AudioService>) -> AppResult<AudioSnapshot> {
    audio.request(Command::Snapshot).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_master_volume(audio: State<'_, AudioService>, volume: f32) -> AppResult<()> {
    let volume = clamp(volume);
    audio
        .request(|reply| Command::SetMasterVolume(volume, reply))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_master_mute(audio: State<'_, AudioService>, muted: bool) -> AppResult<()> {
    audio
        .request(|reply| Command::SetMasterMute(muted, reply))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_app_volume(
    audio: State<'_, AudioService>,
    app_id: String,
    volume: f32,
) -> AppResult<()> {
    let volume = clamp(volume);
    audio
        .request(|reply| Command::SetAppVolume(app_id, volume, reply))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_app_mute(
    audio: State<'_, AudioService>,
    app_id: String,
    muted: bool,
) -> AppResult<()> {
    audio
        .request(|reply| Command::SetAppMute(app_id, muted, reply))
        .await
}

/// 前端首次渲染完成。新建的窗口在此之后才显示，避免出现空白窗口。
#[tauri::command]
#[specta::specta]
pub fn window_ready(app: tauri::AppHandle) {
    crate::window::ready(&app);
}

/// 一次系统音量调节结束（松开滑块、滚轮停止）时调用，按设置播放提示音。
#[tauri::command]
#[specta::specta]
pub fn play_volume_feedback(config: State<'_, Config>, feedback: State<'_, Feedback>) {
    if config.get().volume_feedback {
        feedback.play();
    }
}

/// 窗口所在显示器的刷新率（Hz），读取失败时为 `None`。设置页用它显示默认帧率。
#[tauri::command]
#[specta::specta]
pub fn get_refresh_rate(window: tauri::WebviewWindow) -> Option<u32> {
    let monitor = window.current_monitor().ok().flatten();
    crate::animation::display_refresh_rate(
        monitor.as_ref().and_then(|m| m.name()).map(String::as_str),
    )
}

/// 前端内容高度（逻辑像素）变化时调用，窗口高度随之调整。
/// 返回高度动画的时长（毫秒），立即完成时为 0；动画期间前端隐藏滚动条。
#[tauri::command]
#[specta::specta]
pub fn fit_window_height(window: tauri::WebviewWindow, content_height: f64) -> u32 {
    crate::window::fit_height(&window, content_height)
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(config: State<'_, Config>) -> Settings {
    config.get()
}

/// 当前 Windows 强调色，之后的变化通过 `theme://accent` 事件推送。读取失败时为 `None`。
#[tauri::command]
#[specta::specta]
pub fn get_accent_colors() -> Option<crate::accent::AccentColors> {
    crate::accent::current()
}

/// 是否开机自启（读取系统中的实际注册状态）。
#[tauri::command]
#[specta::specta]
pub fn get_autostart(app: tauri::AppHandle) -> bool {
    crate::autostart::is_enabled(&app)
}

#[tauri::command]
#[specta::specta]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> AppResult<()> {
    crate::autostart::set_enabled(&app, enabled)
}

/// 各设置项的默认值。设置页据此判断是否显示“恢复默认”。
#[tauri::command]
#[specta::specta]
pub fn get_default_settings() -> Settings {
    Settings::default()
}

/// 保存设置并立即生效。窗口隐藏策略在下一次隐藏窗口时生效。
#[tauri::command]
#[specta::specta]
pub fn set_settings(
    window: tauri::WebviewWindow,
    config: State<'_, Config>,
    settings: Settings,
) -> AppResult<()> {
    let result = config.set(settings).map(|_| ());
    // 结束宽度预览，按设置中的值调整；保存失败时即恢复为原来的宽度。
    crate::window::preview_width(&window, None);
    crate::window::apply_theme(&window);
    result
}

/// 设置页拖动宽度滑块时预览窗口宽度（逻辑像素），不写入设置；松手后由 `set_settings` 保存。
#[tauri::command]
#[specta::specta]
pub fn preview_window_width(window: tauri::WebviewWindow, width: u32) {
    crate::window::preview_width(&window, Some(width));
}

/// Windows 要求标量音量在 0–1 之间，超出范围会返回 E_INVALIDARG。
fn clamp(volume: f32) -> f32 {
    if volume.is_nan() {
        0.0
    } else {
        volume.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::clamp;

    #[test]
    fn 音量被限制在0到1之间() {
        assert_eq!(clamp(-0.5), 0.0);
        assert_eq!(clamp(0.42), 0.42);
        assert_eq!(clamp(3.0), 1.0);
        assert_eq!(clamp(f32::NAN), 0.0);
    }
}
