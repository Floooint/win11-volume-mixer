//! 调节系统音量后的提示音，与 Windows 自带音量面板的反馈一致。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::UI::WindowsAndMessaging::MB_OK;

/// 两次提示音的最小间隔。连续调节（拖动、滚轮）时不会响个不停。
const MIN_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Default)]
pub struct Feedback {
    last: Mutex<Option<Instant>>,
}

impl Feedback {
    /// 播放系统“默认提示音”。它按当前系统音量播放，所以音量越大声音越响，
    /// 用户由此感知现在的音量大小。
    pub fn play(&self) {
        let Ok(mut last) = self.last.lock() else {
            return;
        };
        if last.is_some_and(|t| t.elapsed() < MIN_INTERVAL) {
            return;
        }
        *last = Some(Instant::now());
        // SAFETY: MessageBeep 只是请求系统异步播放声音，没有内存安全前提。
        if let Err(e) = unsafe { MessageBeep(MB_OK) } {
            eprintln!("[feedback] 播放提示音失败：{e}");
        }
    }
}
