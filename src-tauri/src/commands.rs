//! Tauri 命令。只负责参数校验和转发，所有音频操作在音频线程上执行。

use tauri::State;

use crate::audio::{AudioService, AudioSnapshot, Command};
use crate::error::AppResult;

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
