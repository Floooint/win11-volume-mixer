//! 任务栏深浅色。托盘图标颜色跟随“Windows 模式”（而非“应用模式”）。

use std::thread;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, RRF_RT_REG_DWORD,
    RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW,
};
use windows_core::w;

const PERSONALIZE: windows_core::PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");

/// 任务栏是否为浅色。读取失败时按深色处理（Windows 11 默认）。
pub fn taskbar_is_light() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PERSONALIZE,
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    result == ERROR_SUCCESS && value != 0
}

/// 在后台线程等待个性化设置变化，每次变化调用 `on_change`。
/// 使用同步的 `RegNotifyChangeKeyValue`，等待期间不占用 CPU。
pub fn watch(on_change: impl Fn() + Send + 'static) {
    let spawned = thread::Builder::new().name("theme".into()).spawn(move || {
        let mut key = HKEY::default();
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PERSONALIZE,
                None,
                KEY_READ | KEY_NOTIFY,
                &mut key,
            )
        };
        if opened != ERROR_SUCCESS {
            eprintln!("[tray] 无法监听深浅色变化：{opened:?}");
            return;
        }
        while unsafe {
            RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, None, false)
        } == ERROR_SUCCESS
        {
            on_change();
        }
        let _ = unsafe { RegCloseKey(key) };
    });
    if let Err(e) = spawned {
        eprintln!("[tray] 无法创建深浅色监听线程：{e}");
    }
}
