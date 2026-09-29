//! 显示 Windows 11 自带的音量浮层（按键盘音量键时出现的那个）。
//!
//! 系统没有公开显示它的接口。做法与 Windhawk 的 taskbar-volume-control 相同：向任务栏中的
//! `MSTaskSwWClass` 窗口投递 `SHELLHOOK` 消息（`HSHELL_APPCOMMAND` + 音量加 / 减），
//! 资源管理器按音量键处理：音量变化 2% 并显示浮层。因此每格滚轮中最后的 2% 交给系统调节，
//! 其余部分由本程序直接调节（见 `wheel.rs`）。与音量键一样，“音量加”会取消静音。

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, PostMessageW, RegisterWindowMessageW,
};
use windows_core::w;

/// `HSHELL_APPCOMMAND` 与 `APPCOMMAND_VOLUME_DOWN` / `APPCOMMAND_VOLUME_UP`，见 WinUser.h。
const HSHELL_APPCOMMAND: usize = 12;
const APPCOMMAND_VOLUME_DOWN: isize = 9;
const APPCOMMAND_VOLUME_UP: isize = 10;

/// 系统调节一次的幅度（2%），与音量键一致。
pub const SYSTEM_STEP: u32 = 2;

/// 让系统把音量调高（`up`）或调低 2% 并显示音量浮层。找不到任务栏窗口时返回 `false`。
pub fn adjust(up: bool) -> bool {
    let Some(target) = task_switcher() else {
        return false;
    };
    let message = unsafe { RegisterWindowMessageW(w!("SHELLHOOK")) };
    if message == 0 {
        return false;
    }
    let command = if up {
        APPCOMMAND_VOLUME_UP
    } else {
        APPCOMMAND_VOLUME_DOWN
    };
    // lParam 的高位字为 app command（低位为设备、按键标志，这里为 0），即 MAKELPARAM(0, command)。
    let result = unsafe {
        PostMessageW(
            Some(target),
            message,
            WPARAM(HSHELL_APPCOMMAND),
            LPARAM(command << 16),
        )
    };
    result.is_ok()
}

/// 任务栏中的任务切换窗口：`Shell_TrayWnd` → `ReBarWindow32` → `MSTaskSwWClass`。
/// Windows 11 的新任务栏仍保留这些隐藏的旧窗口来处理 shell 消息。
fn task_switcher() -> Option<HWND> {
    unsafe {
        let taskbar = FindWindowW(w!("Shell_TrayWnd"), None).ok()?;
        let rebar = FindWindowExW(Some(taskbar), None, w!("ReBarWindow32"), None).ok()?;
        FindWindowExW(Some(rebar), None, w!("MSTaskSwWClass"), None).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 能找到任务栏中的任务切换窗口() {
        // 没有任务栏（如服务器核心版、无桌面的测试环境）时跳过。
        if unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.is_err() {
            return;
        }
        assert!(
            task_switcher().is_some(),
            "任务栏中缺少 MSTaskSwWClass 窗口"
        );
    }
}
