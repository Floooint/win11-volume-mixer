//! 主窗口：定位到托盘附近、显示 / 隐藏、失焦自动隐藏。
//!
//! 隐藏策略由设置项 [`WindowPolicy`] 决定：保留 WebView（打开快）或销毁 WebView（常驻内存低）。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Manager, PhysicalPosition, Runtime, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::config::{Config, WindowPolicy};

pub const MAIN: &str = "main";

/// 窗口与任务栏之间的间距（物理像素，按 DPI 缩放前为 12 px）。
const MARGIN: f64 = 12.0;

/// 点击托盘图标时，窗口会先因失焦而隐藏，紧接着收到点击事件。
/// 在这个时间窗口内的点击视为“关闭”，不再重新打开。
const REOPEN_GUARD: Duration = Duration::from_millis(250);

/// 窗口相关的全局状态。
#[derive(Default)]
pub struct WindowState {
    /// 最近一次因失焦而隐藏的时间。
    hidden_at: Mutex<Option<Instant>>,
    /// 正在创建的窗口：等前端首次渲染完成后再定位并显示，避免白屏闪烁。
    pending: Mutex<Option<Pending>>,
}

struct Pending {
    tray: Option<Rect>,
    requested_at: Instant,
}

fn policy<R: Runtime, M: Manager<R>>(manager: &M) -> WindowPolicy {
    manager.state::<Config>().get().window_policy
}

/// 矩形（物理像素）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// 由显示器区域与工作区的差异判断任务栏所在的边；自动隐藏任务栏时默认为底部。
fn taskbar_edge(monitor: Rect, work_area: Rect) -> Edge {
    let gaps = [
        (Edge::Bottom, monitor.bottom() - work_area.bottom()),
        (Edge::Top, work_area.y - monitor.y),
        (Edge::Left, work_area.x - monitor.x),
        (Edge::Right, monitor.right() - work_area.right()),
    ];
    gaps.into_iter()
        .filter(|(_, gap)| *gap > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(Edge::Bottom, |(edge, _)| edge)
}

/// 计算窗口左上角位置：贴近任务栏，水平（或垂直）方向对齐托盘图标，并限制在工作区内。
pub fn position_near_tray(
    tray: Rect,
    window_width: f64,
    window_height: f64,
    monitor: Rect,
    work_area: Rect,
    scale: f64,
) -> (f64, f64) {
    let margin = MARGIN * scale;
    let clamp_x = |x: f64| {
        x.min(work_area.right() - window_width - margin)
            .max(work_area.x + margin)
    };
    let clamp_y = |y: f64| {
        y.min(work_area.bottom() - window_height - margin)
            .max(work_area.y + margin)
    };
    let center_x = tray.x + tray.width / 2.0 - window_width / 2.0;
    let center_y = tray.y + tray.height / 2.0 - window_height / 2.0;

    match taskbar_edge(monitor, work_area) {
        Edge::Bottom => (
            clamp_x(center_x),
            work_area.bottom() - window_height - margin,
        ),
        Edge::Top => (clamp_x(center_x), work_area.y + margin),
        Edge::Left => (work_area.x + margin, clamp_y(center_y)),
        Edge::Right => (work_area.right() - window_width - margin, clamp_y(center_y)),
    }
}

fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window(MAIN)
}

/// 按 `tauri.conf.json` 中的配置创建主窗口（初始隐藏）。
fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .expect("tauri.conf.json 中缺少主窗口配置");
    WebviewWindowBuilder::from_config(app, config)?.build()
}

/// 启动时调用：保留策略下预先创建窗口，首次打开也能立即显示。
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let policy = policy(app);
    eprintln!("[window] 隐藏策略：{policy:?}");
    if policy == WindowPolicy::Keep {
        create(app)?;
    }
    Ok(())
}

/// 在托盘图标附近显示窗口；`tray` 为 `None` 时（如重复启动）保持原位置。
pub fn show<R: Runtime>(app: &AppHandle<R>, tray: Option<Rect>) {
    let requested_at = Instant::now();
    match main_window(app) {
        Some(window) => {
            // 窗口正在创建、尚未渲染完成时，等 `ready` 统一显示。
            let state = app.state::<WindowState>();
            if let Ok(mut pending) = state.pending.lock()
                && let Some(pending) = pending.as_mut()
            {
                pending.tray = tray.or(pending.tray);
                return;
            }
            present(&window, tray);
            eprintln!(
                "[window] 打开耗时 {:?}（窗口已存在）",
                requested_at.elapsed()
            );
        }
        None => {
            if let Ok(mut pending) = app.state::<WindowState>().pending.lock() {
                *pending = Some(Pending { tray, requested_at });
            }
            if let Err(e) = create(app) {
                eprintln!("[window] 创建窗口失败：{e}");
            }
        }
    }
}

/// 前端首次渲染完成后调用。若窗口是为了打开而新建的，此时再显示。
pub fn ready<R: Runtime>(app: &AppHandle<R>) {
    let pending = app
        .state::<WindowState>()
        .pending
        .lock()
        .ok()
        .and_then(|mut p| p.take());
    if let (Some(pending), Some(window)) = (pending, main_window(app)) {
        present(&window, pending.tray);
        eprintln!(
            "[window] 打开耗时 {:?}（新建窗口）",
            pending.requested_at.elapsed()
        );
    }
}

fn present<R: Runtime>(window: &WebviewWindow<R>, tray: Option<Rect>) {
    if let Some(tray) = tray
        && let Err(e) = move_near_tray(window, tray)
    {
        eprintln!("[window] 定位失败：{e}");
    }
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn hide<R: Runtime>(window: &tauri::Window<R>) {
    match policy(window) {
        WindowPolicy::Keep => {
            let _ = window.hide();
        }
        WindowPolicy::Destroy => {
            let _ = window.destroy();
        }
    }
}

fn move_near_tray<R: Runtime>(window: &WebviewWindow<R>, tray: Rect) -> tauri::Result<()> {
    let Some(monitor) = window
        .monitor_from_point(tray.x + tray.width / 2.0, tray.y + tray.height / 2.0)?
        .or(window.primary_monitor()?)
    else {
        return Ok(());
    };
    let size = window.outer_size()?;
    let to_rect = |pos: PhysicalPosition<i32>, width: u32, height: u32| Rect {
        x: pos.x.into(),
        y: pos.y.into(),
        width: width.into(),
        height: height.into(),
    };
    let area = monitor.work_area();
    let (x, y) = position_near_tray(
        tray,
        size.width.into(),
        size.height.into(),
        to_rect(
            *monitor.position(),
            monitor.size().width,
            monitor.size().height,
        ),
        to_rect(area.position, area.size.width, area.size.height),
        monitor.scale_factor(),
    );
    window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
}

/// 托盘左键点击：窗口可见则隐藏，否则在托盘附近显示。
pub fn toggle<R: Runtime>(app: &AppHandle<R>, tray: Rect) {
    let just_hidden = app
        .state::<WindowState>()
        .hidden_at
        .lock()
        .ok()
        .and_then(|mut t| t.take())
        .is_some_and(|t| t.elapsed() < REOPEN_GUARD);

    match main_window(app) {
        Some(window) if window.is_visible().unwrap_or(false) => hide(&window.as_ref().window()),
        _ if !just_hidden => show(app, Some(tray)),
        _ => {}
    }
}

/// 失焦自动隐藏；关闭请求（如 Alt+F4）改为隐藏，程序继续在托盘运行。
pub fn handle_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    match event {
        // 新建窗口在渲染完成前不可见，此时的失焦事件忽略。
        WindowEvent::Focused(false) if window.is_visible().unwrap_or(false) => {
            if let Ok(mut t) = window.state::<WindowState>().hidden_at.lock() {
                *t = Some(Instant::now());
            }
            hide(window);
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(window);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 380.0;
    const H: f64 = 520.0;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// 1920×1080 显示器。
    const MONITOR: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    #[test]
    fn 任务栏在底部时窗口贴在托盘图标上方() {
        let work = rect(0.0, 0.0, 1920.0, 1032.0);
        let tray = rect(1700.0, 1040.0, 32.0, 32.0);
        let (x, y) = position_near_tray(tray, W, H, MONITOR, work, 1.0);
        assert_eq!(x, 1716.0 - W / 2.0);
        assert_eq!(y, 1032.0 - H - 12.0);
    }

    #[test]
    fn 托盘靠近屏幕右缘时窗口不超出工作区() {
        let work = rect(0.0, 0.0, 1920.0, 1032.0);
        let tray = rect(1880.0, 1040.0, 32.0, 32.0);
        let (x, _) = position_near_tray(tray, W, H, MONITOR, work, 1.0);
        assert_eq!(x, 1920.0 - W - 12.0);
    }

    #[test]
    fn 任务栏在顶部时窗口贴在工作区顶部() {
        let work = rect(0.0, 48.0, 1920.0, 1032.0);
        let tray = rect(1700.0, 8.0, 32.0, 32.0);
        let (_, y) = position_near_tray(tray, W, H, MONITOR, work, 1.0);
        assert_eq!(y, 48.0 + 12.0);
    }

    #[test]
    fn 任务栏在左侧时窗口贴在工作区左侧并对齐图标() {
        let work = rect(64.0, 0.0, 1856.0, 1080.0);
        let tray = rect(16.0, 900.0, 32.0, 32.0);
        let (x, y) = position_near_tray(tray, W, H, MONITOR, work, 1.0);
        assert_eq!(x, 64.0 + 12.0);
        assert_eq!(y, 1080.0 - H - 12.0, "图标靠近底部时被限制在工作区内");
    }

    #[test]
    fn 任务栏自动隐藏时按底部处理() {
        let tray = rect(1700.0, 1040.0, 32.0, 32.0);
        let (_, y) = position_near_tray(tray, W, H, MONITOR, MONITOR, 1.0);
        assert_eq!(y, 1080.0 - H - 12.0);
    }

    #[test]
    fn 间距随缩放比例放大() {
        let work = rect(0.0, 0.0, 3840.0, 2064.0);
        let monitor = rect(0.0, 0.0, 3840.0, 2160.0);
        let tray = rect(3400.0, 2080.0, 64.0, 64.0);
        let (_, y) = position_near_tray(tray, W, H, monitor, work, 2.0);
        assert_eq!(y, 2064.0 - H - 24.0);
    }

    #[test]
    fn 副显示器坐标偏移时仍在该显示器内定位() {
        let monitor = rect(1920.0, 0.0, 1920.0, 1080.0);
        let work = rect(1920.0, 0.0, 1920.0, 1032.0);
        let tray = rect(3700.0, 1040.0, 32.0, 32.0);
        let (x, y) = position_near_tray(tray, W, H, monitor, work, 1.0);
        assert!(x >= 1920.0 && x + W <= 3840.0);
        assert_eq!(y, 1032.0 - H - 12.0);
    }
}
