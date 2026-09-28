//! 音频模块。所有 COM 对象只在音频线程上创建和调用，
//! COM 回调只把事件转成 [`Msg`] 发回音频线程。

mod app_info;
mod device;
mod session;
mod thread;

pub use thread::spawn;

use windows::Win32::Media::Audio::AudioSessionState;
use windows::Win32::System::Com::CoTaskMemFree;
use windows_core::{GUID, PWSTR};

/// 本程序发起的音量修改都带上这个标识，回调据此区分自身修改和外部修改。
pub const EVENT_CONTEXT: GUID = GUID::from_u128(0x6f1c2b9e_4a53_4d8e_9c1a_7e3b5d20a41f);

/// 前端（这里是命令行）发给音频线程的请求。
#[derive(Debug)]
pub enum Command {
    List,
    Refresh,
    SetMasterVolume(f32),
    SetMasterMute(bool),
    /// 应用按上一次 `List` 输出的序号选择。
    SetAppVolume(usize, f32),
    SetAppMute(usize, bool),
    Shutdown,
}

/// 音频线程的消息队列：请求和 COM 回调事件共用一个通道。
pub enum Msg {
    Command(Command),
    MasterChanged {
        volume: f32,
        muted: bool,
        is_self: bool,
    },
    SessionCreated,
    SessionVolumeChanged {
        session_id: String,
        volume: f32,
        muted: bool,
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

pub(crate) fn percent(volume: f32) -> String {
    format!("{:.0}%", volume * 100.0)
}
