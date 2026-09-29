//! 滚轮调节系统音量：托盘图标上，以及（设置项 `taskbarWheel` 开启时）整个任务栏上。
//!
//! 托盘图标和任务栏都不会把滚轮消息交给本程序，因此注册全局 Raw Input（`RIDEV_INPUTSINK`，
//! 程序不在前台也能收到）：
//! - 只有托盘图标时，鼠标移入图标才注册，移出图标区域后立即注销，平时不监听任何全局输入；
//! - 开启任务栏滚轮后一直注册，收到滚轮时判断光标下的窗口是否属于任务栏。鼠标移动也会送达，
//!   但只读取一次输入数据就返回，不做其他处理；鼠标不动时没有任何开销。
//!
//! 每格调节的幅度、提示音和是否显示系统音量浮层由设置项 `wheelStep` / `wheelFeedback` /
//! `wheelOsd` 决定（见 `osd.rs`）。
//!
//! 不用 `WH_MOUSE_LL` 钩子：前台窗口是管理员权限等高完整性进程时，钩子收不到输入。
//! EarTrumpet 使用同样的做法。接收窗口、注册和注销都在主线程上。

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GA_ROOT, GetAncestor, GetClassNameW, GetCursorPos,
    HWND_MESSAGE, RI_MOUSE_WHEEL, RegisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT,
    WNDCLASSW, WindowFromPoint,
};
use windows_core::w;

use super::osd;
use crate::audio::{AudioService, Command};
use crate::config::Config;
use crate::feedback::Feedback;
use crate::window::Rect;

/// 标准滚轮一格的增量；高精度滚轮每次给出更小的值，累积到一格再调节。
const WHEEL_DELTA: i32 = 120;
/// 滚轮停止多久后播放提示音，与窗口内一致。
const FEEDBACK_DELAY: Duration = Duration::from_millis(250);

/// 通用桌面控件 / 鼠标，见 HID Usage Tables。
const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

/// 主任务栏和其他显示器上的任务栏的窗口类名。
const TASKBAR_CLASSES: [&str; 2] = ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

struct Listener {
    /// 接收 `WM_INPUT` 的消息专用窗口，首次使用时创建，之后一直保留。
    hwnd: HWND,
    app: AppHandle,
    /// 托盘图标区域（物理像素），与 `GetCursorPos` 的坐标一致。鼠标不在图标上时为 `None`。
    tray: Option<Rect>,
    /// 任务栏滚轮已开启：一直保持注册。
    taskbar: bool,
    registered: bool,
    /// 尚未凑满一格的滚轮增量。
    remainder: i32,
}

thread_local! {
    static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
}

/// 每次滚动加一，延迟播放提示音时据此判断滚轮是否已停止。
static WHEEL_SEQ: AtomicU64 = AtomicU64::new(0);

/// 取得（必要时创建）监听器并执行 `f`。只能在主线程调用。
fn with_listener(app: &AppHandle, f: impl FnOnce(&mut Listener)) {
    LISTENER.with_borrow_mut(|listener| {
        if listener.is_none() {
            match create_window() {
                Ok(hwnd) => {
                    *listener = Some(Listener {
                        hwnd,
                        app: app.clone(),
                        tray: None,
                        taskbar: false,
                        registered: false,
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
            f(listener);
        }
    });
}

/// 鼠标进入或在托盘图标上移动。
pub fn hover(app: &AppHandle, rect: Rect) {
    with_listener(app, |listener| {
        listener.tray = Some(rect);
        listener.update();
    });
}

/// 鼠标离开托盘图标。托盘的离开事件并不总能及时送达，收到输入时也会自行检查。
pub fn leave() {
    LISTENER.with_borrow_mut(|listener| {
        if let Some(listener) = listener {
            listener.tray = None;
            listener.update();
        }
    });
}

/// 开启或关闭任务栏滚轮。只能在主线程调用。
pub fn set_taskbar(app: &AppHandle, enabled: bool) {
    with_listener(app, |listener| {
        listener.taskbar = enabled;
        listener.update();
    });
}

impl Listener {
    /// 需要监听（鼠标在托盘图标上，或开启了任务栏滚轮）时注册，否则注销。
    fn update(&mut self) {
        let wanted = self.taskbar || self.tray.is_some();
        if wanted == self.registered {
            return;
        }
        let device = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: if wanted {
                RIDEV_INPUTSINK
            } else {
                RIDEV_REMOVE
            },
            hwndTarget: if wanted { self.hwnd } else { HWND::default() },
        };
        match unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) } {
            Ok(()) => self.registered = wanted,
            Err(e) => eprintln!("[tray] 无法注册 / 注销滚轮输入：{e}"),
        }
        self.remainder = 0;
    }

    /// 处理一次鼠标输入。只发送请求，不等待结果。
    fn on_input(&mut self, wheel_delta: Option<i32>) {
        // 只开启了托盘图标滚轮时，每次输入都检查光标是否仍在图标上，移出即注销。
        let on_tray = || {
            self.tray
                .is_some_and(|rect| cursor().is_some_and(|pt| contains(&rect, pt)))
        };
        if !self.taskbar && !on_tray() {
            self.tray = None;
            self.update();
            return;
        }
        let Some(delta) = wheel_delta else {
            return;
        };
        // 开启任务栏滚轮时，只在滚轮事件上判断位置，鼠标移动不做任何处理。
        if self.taskbar {
            let Some(pt) = cursor() else {
                return;
            };
            let on_tray = self.tray.is_some_and(|rect| contains(&rect, pt));
            if !on_tray && !over_taskbar(pt) {
                self.tray = None;
                self.remainder = 0;
                return;
            }
        }
        let notches = take_steps(&mut self.remainder, delta);
        if notches == 0 {
            return;
        }
        let (step, feedback, show_osd) = self
            .app
            .state::<Config>()
            .read(|s| (s.wheel_step, s.wheel_feedback, s.wheel_osd));
        let (direct, system) = split(notches, step, show_osd);
        if system == 0 {
            self.app
                .state::<AudioService>()
                .post(|reply| Command::AdjustMasterVolume(direct as f32 / 100.0, reply));
        } else {
            adjust_with_osd(self.app.clone(), direct, system);
        }
        if feedback {
            schedule_feedback(self.app.clone());
        }
    }
}

fn cursor() -> Option<POINT> {
    let mut pt = POINT::default();
    unsafe { GetCursorPos(&mut pt) }.ok().map(|()| pt)
}

/// 光标下的窗口是否属于任务栏（主任务栏或其他显示器上的任务栏）。
/// `WindowFromPoint` 不受其他进程权限的限制，前台是管理员权限窗口时也能判断。
fn over_taskbar(pt: POINT) -> bool {
    let root = unsafe { GetAncestor(WindowFromPoint(pt), GA_ROOT) };
    if root.is_invalid() {
        return false;
    }
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(root, &mut class) };
    let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
    is_taskbar_class(&class)
}

fn is_taskbar_class(class: &str) -> bool {
    TASKBAR_CLASSES.contains(&class)
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
    // 用异步任务等待，快速滚动时不会每格都新建一个系统线程。
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FEEDBACK_DELAY).await;
        if WHEEL_SEQ.load(Ordering::SeqCst) == seq {
            app.state::<Feedback>().play();
        }
    });
}

fn contains(rect: &Rect, pt: POINT) -> bool {
    let (x, y) = (pt.x as f64, pt.y as f64);
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

/// 先直接调节 `direct` 个百分点，完成后再让系统调节 `system`（±1）次 2% 并显示浮层。
/// 必须等直接调节完成：步长为 1% 时两部分方向相反（-1% 再 +2%），若系统先调，
/// 在 0% 或 100% 处会被截断，结果出错。找不到任务栏窗口时这 2% 也直接调节。
fn adjust_with_osd(app: AppHandle, direct: i32, system: i32) {
    tauri::async_runtime::spawn(async move {
        let audio = app.state::<AudioService>();
        if direct != 0 {
            let _ = audio
                .request(|reply| Command::AdjustMasterVolume(direct as f32 / 100.0, reply))
                .await;
        }
        if !osd::adjust(system > 0) {
            let fallback = (system * osd::SYSTEM_STEP as i32) as f32 / 100.0;
            audio.post(|reply| Command::AdjustMasterVolume(fallback, reply));
        }
    });
}

/// 把 `notches` 格滚轮（向上为正）、每格 `step`% 的调节分成两部分：
/// 本程序直接调节的百分点，以及交给系统调节（每次 2%，同时显示音量浮层）的次数。
/// 与 Windhawk 的做法一致，每次滚动只让系统调节一次，其余直接调节，避免浮层反复刷新。
/// 总量不足 2% 时（步长 1%）直接调节部分为反方向，如 +1% = -1% + 系统 +2%。
fn split(notches: i32, step: u32, show_osd: bool) -> (i32, i32) {
    let total = notches * step as i32;
    if !show_osd || total == 0 {
        return (total, 0);
    }
    let system = total.signum();
    (total - system * osd::SYSTEM_STEP as i32, system)
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

    #[test]
    fn 按步长和浮层设置拆分调节量() {
        assert_eq!(split(1, 2, false), (2, 0));
        assert_eq!(
            split(-3, 5, false),
            (-15, 0),
            "不显示浮层时任意步长都直接调节"
        );
        assert_eq!(split(1, 2, true), (0, 1), "2% 全部交给系统");
        assert_eq!(split(1, 1, true), (-1, 1), "1% = -1% + 系统 +2%");
        assert_eq!(split(-1, 1, true), (1, -1));
        assert_eq!(split(3, 1, true), (1, 1));
        assert_eq!(split(1, 6, true), (4, 1));
        assert_eq!(split(-2, 4, true), (-6, -1));
        assert_eq!(split(0, 4, true), (0, 0));
    }

    #[test]
    fn 识别主任务栏和副屏任务栏() {
        assert!(is_taskbar_class("Shell_TrayWnd"));
        assert!(is_taskbar_class("Shell_SecondaryTrayWnd"));
        assert!(!is_taskbar_class("Progman"));
        assert!(
            !is_taskbar_class("shell_traywnd"),
            "类名区分大小写，与系统一致"
        );
    }

    #[test]
    fn 光标下为任务栏时能识别() {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};
        assert!(
            !over_taskbar(POINT {
                x: -100000,
                y: -100000
            }),
            "屏幕外不是任务栏"
        );
        // 取任务栏窗口的中心点。没有任务栏（如服务器核心版）或任务栏自动隐藏时跳过。
        let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else {
            return;
        };
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(taskbar, &mut rect) }.is_err() || rect.bottom - rect.top < 8 {
            return;
        }
        let center = POINT {
            x: (rect.left + rect.right) / 2,
            y: (rect.top + rect.bottom) / 2,
        };
        // 其他置顶窗口可能恰好盖住中心点，只在确实取到任务栏时断言。
        let root = unsafe { GetAncestor(WindowFromPoint(center), GA_ROOT) };
        if root == taskbar {
            assert!(over_taskbar(center));
        }
    }
}
