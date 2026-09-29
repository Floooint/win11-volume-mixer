//! 音频模块。所有 COM 对象只在音频线程上创建和调用，
//! COM 回调只把事件转成 [`Msg`] 发回音频线程。规则见 docs/architecture.md。

mod aggregate;
mod app_info;
mod device;
mod session;
mod thread;
mod types;

pub use thread::AudioService;
use tokio::sync::oneshot;
pub use types::{AppAudio, AudioSnapshot, VolumeState};
use windows::Win32::Media::Audio::AudioSessionState;
use windows::Win32::System::Com::CoTaskMemFree;
use windows_core::{GUID, PWSTR};

use crate::error::AppResult;

/// 本程序发起的音量修改都带上这个标识，回调据此区分自身修改和外部修改。
pub const EVENT_CONTEXT: GUID = GUID::from_u128(0x6f1c2b9e_4a53_4d8e_9c1a_7e3b5d20a41f);

type Reply<T> = oneshot::Sender<AppResult<T>>;

/// 音频线程推送出去的变化。
#[derive(Debug)]
pub enum Update {
    Snapshot(AudioSnapshot),
    Master(VolumeState),
    AppUpsert(AppAudio),
    AppRemove(String),
    /// 系统音量的当前状态，供托盘图标使用。与 `Master` 不同，本程序自身的修改也会推送。
    /// 没有输出设备时为 `None`。
    MasterStatus(Option<MasterStatus>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MasterStatus {
    pub device_name: String,
    pub volume: VolumeState,
}

/// Tauri 命令发给音频线程的请求。
pub enum Command {
    Snapshot(Reply<AudioSnapshot>),
    SetMasterVolume(f32, Reply<()>),
    SetMasterMute(bool, Reply<()>),
    /// 在当前系统音量上增减，用于托盘滚轮。结果会推送给前端。
    AdjustMasterVolume(f32, Reply<()>),
    /// 切换系统静音，用于托盘中键。结果会推送给前端。
    ToggleMasterMute(Reply<()>),
    SetAppVolume(String, f32, Reply<()>),
    SetAppMute(String, bool, Reply<()>),
    Shutdown,
}

/// 音频线程的消息队列：请求和 COM 回调事件共用一个通道。
pub enum Msg {
    Command(Command),
    MasterChanged {
        is_self: bool,
    },
    SessionCreated,
    SessionVolumeChanged {
        is_self: bool,
    },
    SessionStateChanged {
        session_id: String,
        state: AudioSessionState,
    },
    SessionDisconnected {
        session_id: String,
    },
    DefaultDeviceChanged,
}

/// 读取由 COM 分配的字符串并释放内存。
///
/// # Safety
/// `p` 必须是 `CoTaskMemAlloc` 分配的以 0 结尾的字符串，调用后不能再使用。
pub(crate) unsafe fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}
