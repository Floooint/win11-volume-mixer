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
    /// 可用的输出设备（不含已断开、已禁用的），按名称排序。
    pub outputs: Vec<OutputDevice>,
    /// 已按“活跃在前，再按名称”排序。
    pub apps: Vec<AppAudio>,
}

/// 输出设备的类型，界面据此显示图标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OutputKind {
    Speakers,
    Headphones,
    /// 显示器（HDMI / DisplayPort）。
    Display,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub kind: OutputKind,
    /// 是默认设备（`eConsole` 角色）。
    pub is_default: bool,
    /// 是默认通信设备（`eCommunications` 角色）。
    pub is_communications: bool,
}

/// 切换默认输出设备时设置哪些角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OutputRole {
    /// 默认设备和默认通信设备。
    All,
    /// 只设为默认设备。
    Default,
    /// 只设为默认通信设备。
    Communications,
}
