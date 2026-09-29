//! 推送给前端的事件，由 tauri-specta 生成对应的 TS 监听函数。

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::audio::{AppAudio, AudioSnapshot, Update, VolumeState};

/// 设备切换或需要整体重建时推送完整状态。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "audio://snapshot")]
pub struct AudioSnapshotEvent(pub AudioSnapshot);

/// 系统总音量被外部修改。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "audio://master")]
pub struct MasterChangedEvent(pub VolumeState);

/// 应用出现或状态变化。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "audio://app-upsert")]
pub struct AppUpsertEvent(pub AppAudio);

/// 应用的最后一个会话退出。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "audio://app-remove")]
pub struct AppRemoveEvent {
    pub app_id: String,
}

pub fn emit(app: &AppHandle, update: Update) {
    let result = match update {
        Update::Snapshot(snapshot) => AudioSnapshotEvent(snapshot).emit(app),
        Update::Master(master) => MasterChangedEvent(master).emit(app),
        Update::AppUpsert(audio) => AppUpsertEvent(audio).emit(app),
        Update::AppRemove(app_id) => AppRemoveEvent { app_id }.emit(app),
        // 只用于托盘图标，由 lib.rs 转给 tray 模块，不推送给前端。
        Update::MasterStatus(_) => Ok(()),
    };
    if let Err(e) = result {
        eprintln!("[events] 推送失败：{e}");
    }
}
