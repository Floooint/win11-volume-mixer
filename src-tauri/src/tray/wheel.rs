//! 托盘图标上的滚轮调节系统音量。
//!
//! 托盘图标收不到滚轮消息。鼠标移入图标时注册全局 Raw Input（`RIDEV_INPUTSINK`，
//! 程序不在前台也能收到），鼠标移出图标区域后立即注销，平时不监听任何全局输入。
//! 不用 `WH_MOUSE_LL` 钩子：前台窗口是管理员权限等高完整性进程时，钩子收不到输入。
//! EarTrumpet 使用同样的做法。接收窗口、注册和注销都在主线程上。

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetCursorPos, HWND_MESSAGE, RI_MOUSE_WHEEL, RegisterClassW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WNDCLASSW,
};
use windows_core::w;

use crate::audio::{AudioService, Command};
use crate::feedback::Feedback;
use crate::window::Rect;

/// 每格滚轮调整的音量，与窗口内滚轮一致。
const STEP: f32 = 0.02;
/// 标准滚轮一格的增量；高精度滚轮每次给出更小的值，累积到一格再调节。
const WHEEL_DELTA: i32 = 120;
/// 滚轮停止多久后播放提示音，与窗口内一致。
const FEEDBACK_DELAY: Duration = Duration::from_millis(250);

/// 通用桌面控件 / 鼠标，见 HID Usage Tables。
const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

struct Listener {
    /// 接收 `WM_INPUT` 的消息专用窗口，首次使用时创建，之后一直保留。
    hwnd: HWND,
    app: AppHandle,
    /// 托盘图标区域（物理像素），与 `GetCursorPos` 的坐标一致。
    rect: Rect,
    listening: bool,
    /// 尚未凑满一格的滚轮增量。
    remainder: i32,
}

thread_local! {
    static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
}

/// 每次滚动加一，延迟播放提示音时据此判断滚轮是否已停止。
static WHEEL_SEQ: AtomicU64 = AtomicU64::new(0);

/// 鼠标进入或在图标上移动。
pub fn hover(app: &AppHandle, rect: Rect) {
    LISTENER.with_borrow_mut(|listener| {
        if listener.is_none() {
            match create_window() {
                Ok(hwnd) => {
                    *listener = Some(Listener {
                        hwnd,
                        app: app.clone(),
                        rect,
                        listening: false,
                        remainder: 0,
                    })
                }
                Err(e) => {
                    eprintln!("[tray] 无法创建滚轮接收窗口：{e}");
                    return;
                }
            }
        }
        if let Some(listener) = listener {
            listener.rect = rect;
            listener.start();
        }
    });
}

/// 鼠标离开图标。托盘的离开事件并不总能及时送达，收到输入时也会自行检查。
pub fn leave() {
    LISTENER.with_borrow_mut(|listener| {
        if let Some(listener) = listener {
            listener.stop();
        }
    });
}

impl Listener {
    fn start(&mut self) {
        if self.listening {
            return;
        }
        let device = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: self.hwnd,
        };
        match unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) } {
            Ok(()) => self.listening = true,
            Err(e) => eprintln!("[tray] 无法注册滚轮输入：{e}"),
        }
    }

    fn stop(&mut self) {
        if !self.listening {
            return;
        }
        let device = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: RIDEV_REMOVE,
            hwndTarget: HWND::default(),
        };
        let _ = unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) };
        self.listening = false;
        self.remainder = 0;
    }

    /// 处理一次鼠标输入。只发送请求，不等待结果。
    fn on_input(&mut self, wheel_delta: Option<i32>) {
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) }.is_err() || !contains(&self.rect, cursor) {
            self.stop();
            return;
        }
        let Some(delta) = wheel_delta else {
            return;
        };
        let steps = take_steps(&mut self.remainder, delta);
        if steps != 0 {
            self.app
                .state::<AudioService>()
                .post(|reply| Command::AdjustMasterVolume(steps as f32 * STEP, reply));
            schedule_feedback(self.app.clone());
        }
    }
}

fn create_window() -> windows_core::Result<HWND> {
    let class = w!("Win11VolumeMixerTrayWheel");
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        // 只在首次创建时注册一次，失败说明类已存在，交给 CreateWindowExW 判断。
        RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            None,
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_INPUT
        && let Some(wheel) = read_mouse(HRAWINPUT(lparam.0 as *mut c_void))
    {
        let _ = LISTENER.try_with(|listener| {
            if let Ok(mut listener) = listener.try_borrow_mut()
                && let Some(listener) = listener.as_mut()
            {
                listener.on_input(wheel);
            }
        });
    }
    // WM_INPUT 也必须交给 DefWindowProc，系统才会释放输入数据。
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// 读取一次鼠标输入：不是鼠标输入时返回 `None`；是鼠标输入时返回滚轮增量（没有滚动为 `None`）。
fn read_mouse(handle: HRAWINPUT) -> Option<Option<i32>> {
    // SAFETY: RAWINPUT 足以容纳鼠标输入，GetRawInputData 按 size 写入不超出缓冲区。
    let mut raw: RAWINPUT = unsafe { std::mem::zeroed() };
    let mut size = size_of::<RAWINPUT>() as u32;
    let read = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            Some(&mut raw as *mut RAWINPUT as *mut c_void),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX || read == 0 || raw.header.dwType != RIM_TYPEMOUSE.0 {
        return None;
    }
    // SAFETY: dwType 为鼠标时 data 中有效的是 mouse。
    let buttons = unsafe { raw.data.mouse.Anonymous.Anonymous };
    let wheel = (buttons.usButtonFlags as u32 & RI_MOUSE_WHEEL != 0)
        // 滚动量为有符号数，向上为正。
        .then_some(buttons.usButtonData as i16 as i32);
    Some(wheel)
}

/// 滚轮停止后播放一次提示音，与窗口内滚轮调节的反馈一致。
fn schedule_feedback(app: AppHandle) {
    let seq = WHEEL_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = thread::Builder::new()
        .name("tray-feedback".into())
        .spawn(move || {
            thread::sleep(FEEDBACK_DELAY);
            if WHEEL_SEQ.load(Ordering::SeqCst) == seq {
                app.state::<Feedback>().play();
            }
        });
}

fn contains(rect: &Rect, pt: POINT) -> bool {
    let (x, y) = (pt.x as f64, pt.y as f64);
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

/// 累积滚轮增量，返回凑满的格数（向上为正），余数留到下次。
fn take_steps(remainder: &mut i32, delta: i32) -> i32 {
    *remainder += delta;
    let steps = *remainder / WHEEL_DELTA;
    *remainder %= WHEEL_DELTA;
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 标准滚轮每格调节一次() {
        let mut remainder = 0;
        assert_eq!(take_steps(&mut remainder, 120), 1);
        assert_eq!(take_steps(&mut remainder, -240), -2);
        assert_eq!(remainder, 0);
    }

    #[test]
    fn 高精度滚轮小增量累积到一格才调节() {
        let mut remainder = 0;
        assert_eq!(take_steps(&mut remainder, 50), 0);
        assert_eq!(take_steps(&mut remainder, 50), 0);
        assert_eq!(take_steps(&mut remainder, 50), 1);
        assert_eq!(remainder, 30);
        // 反向滚动先抵消余数。
        assert_eq!(take_steps(&mut remainder, -40), 0);
        assert_eq!(remainder, -10);
    }

    #[test]
    fn 只处理图标区域内的滚动() {
        let rect = Rect {
            x: 100.0,
            y: 50.0,
            width: 24.0,
            height: 40.0,
        };
        assert!(contains(&rect, POINT { x: 100, y: 50 }));
        assert!(contains(&rect, POINT { x: 123, y: 89 }));
        assert!(!contains(&rect, POINT { x: 124, y: 60 }));
        assert!(!contains(&rect, POINT { x: 110, y: 49 }));
    }
}
