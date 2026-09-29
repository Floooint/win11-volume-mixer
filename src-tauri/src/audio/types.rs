//! 前后端共享的数据类型，由 tauri-specta 导出到 `src/bindings.ts`。
//! 音量统一使用 `0.0..=1.0` 的标量，界面显示时换算为 0–100。

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeState {
    pub volume: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// 设备的系统总音量。
    pub master: VolumeState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppAudio {
    pub app_id: String,
    pub name: String,
    /// 图标来源，前端通过 `appicon` 协议加载（`convertFileSrc(icon, "appicon")`）。
    /// 无法确定来源时（如管理员权限进程）为 `None`，前端显示首字母占位。
    pub icon: Option<String>,
    /// 进程的 exe 文件名，右键菜单“复制进程名”使用。系统声音为 `None`。
    pub process_name: Option<String>,
    /// exe 完整路径，右键菜单“复制路径”使用。打不开进程时为 `None`。
    pub exe_path: Option<String>,
    pub volume: VolumeState,
    /// 是否有会话正在播放。
    pub active: bool,
    pub session_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioSnapshot {
    /// 没有任何输出设备时为 `None`。
    pub device: Option<DeviceInfo>,
    /// 已按“活跃在前，再按名称”排序。
    pub apps: Vec<AppAudio>,
}
