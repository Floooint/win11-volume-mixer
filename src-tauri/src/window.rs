//! 主窗口：定位到托盘附近、显示 / 隐藏、失焦自动隐藏。
//!
//! 隐藏后的行为由设置项 [`WindowPolicy`] 决定：常驻（保留界面）、静默（立即释放界面）、
//! 智能（保留一段时间，超时未打开再释放）。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tauri::async_runtime::JoinHandle;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::animation::{self, Generation};
use crate::config::{Config, WindowPolicy};

pub const MAIN: &str = "main";

/// 窗口与任务栏之间的间距（物理像素，按 DPI 缩放前为 12 px）。
const MARGIN: f64 = 12.0;

/// 窗口最小高度（逻辑像素）。
const MIN_HEIGHT: f64 = 160.0;

/// 窗口最大高度占屏幕高度的比例。
const MAX_SCREEN_RATIO: f64 = 0.8;

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
    /// 智能模式下的释放计时器；再次打开窗口时取消。
    release_timer: Mutex<Option<JoinHandle<()>>>,
    /// 最近一次托盘图标位置，用于高度变化后重新定位，以及没有托盘位置的打开请求。
    last_tray: Mutex<Option<Rect>>,
    /// 滑出动画代数，打开窗口时递增以取消进行中的动画。
    animation: Generation,
    /// 是否正在播放滑出动画。
    hiding: AtomicBool,
}

struct Pending {
    tray: Option<Rect>,
    requested_at: Instant,
}

fn policy<R: Runtime, M: Manager<R>>(manager: &M) -> WindowPolicy {
    manager.state::<Config>().get().window_policy
}

fn fps<R: Runtime, M: Manager<R>>(manager: &M) -> u32 {
    manager.state::<Config>().get().animation_fps
}

fn cancel_release_timer<R: Runtime, M: Manager<R>>(manager: &M) {
    let timer = manager
        .state::<WindowState>()
        .release_timer
        .lock()
        .ok()
        .and_then(|mut t| t.take());
    if let Some(timer) = timer {
        timer.abort();
    }
}

/// 智能模式：窗口隐藏后开始计时，到期仍未打开则释放界面。
fn start_release_timer<R: Runtime>(window: &tauri::Window<R>) {
    cancel_release_timer(window);
    let seconds = window.state::<Config>().get().smart_release_seconds;
    let app = window.app_handle().clone();
    let timer = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(u64::from(seconds))).await;
        // 计时期间用户可能改了模式，到期时再确认一次。
        if policy(&app) == WindowPolicy::Smart
            && let Some(window) = main_window(&app)
            && !window.is_visible().unwrap_or(true)
        {
            eprintln!("[window] 智能模式：{seconds} 秒未打开，释放界面");
            let _ = window.destroy();
        }
    });
    if let Ok(mut t) = window.state::<WindowState>().release_timer.lock() {
        *t = Some(timer);
    }
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
    let window = WebviewWindowBuilder::from_config(app, config)?.build()?;
    disable_system_transitions(&window);
    Ok(window)
}

/// 关闭 Windows 自带的窗口显示 / 隐藏过渡动画（淡入淡出、缩放），
/// 否则它会叠加在自定义的滑入 / 滑出动画之后，出现“残影”。
fn disable_system_transitions<R: Runtime>(window: &WebviewWindow<R>) {
    use windows::Win32::Graphics::Dwm::{DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute};
    use windows_core::BOOL;

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let disabled = BOOL::from(true);
    // SAFETY: hwnd 来自刚创建的窗口；传入 BOOL 的地址和大小符合该属性的要求。
    let result = unsafe {
        DwmSetWindowAttribute(
            windows::Win32::Foundation::HWND(hwnd.0),
            DWMWA_TRANSITIONS_FORCEDISABLED,
            (&raw const disabled).cast(),
            size_of::<BOOL>() as u32,
        )
    };
    if let Err(e) = result {
        eprintln!("[window] 关闭系统过渡动画失败：{e}");
    }
}

/// 启动时调用：常驻和智能模式下预先创建窗口，首次打开也能立即显示。
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let policy = policy(app);
    eprintln!("[window] 运行模式：{policy:?}");
    if policy != WindowPolicy::Silent {
        create(app)?;
    }
    Ok(())
}

/// 在托盘图标附近显示窗口；`tray` 为 `None` 时（如重复启动）使用上次的托盘位置。
pub fn show<R: Runtime>(app: &AppHandle<R>, tray: Option<Rect>) {
    let requested_at = Instant::now();
    cancel_release_timer(app);
    cancel_hide_animation(app);
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
    let tray = remember_tray(window, tray);
    // 用计算出的目标位置，而不是移动后再读回：非主线程上 set_position 是异步的，读回可能是旧值。
    let target = match tray.map(|tray| target_near_tray(window, tray)) {
        Some(Ok(Some(target))) => Some(target),
        Some(Err(e)) => {
            eprintln!("[window] 定位失败：{e}");
            None
        }
        _ => None,
    }
    .or_else(|| window.outer_position().ok());
    let Some(target) = target else {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    };
    let offset = slide_distance(window);

    // 先放到目标位置下方再显示，然后滑到目标位置。
    let _ = window.set_position(PhysicalPosition::new(target.x, target.y + offset));
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();

    let generation = window.state::<WindowState>().animation.next();
    let window = window.clone();
    std::thread::spawn(move || {
        let state = window.state::<WindowState>();
        let completed = animation::slide(
            animation::ENTER,
            fps(&window),
            target.y + offset,
            target.y,
            || state.animation.is_current(generation),
            |y| {
                let _ = window.set_position(PhysicalPosition::new(target.x, y));
            },
        );
        // 被新的动画取消时不修正位置，交给新动画处理。
        if completed {
            let _ = window.set_position(target);
        }
    });
}

/// 滑入的起始偏移：窗口高度的一小段，避免窗口从屏幕底部整段飞入。
fn slide_distance<R: Runtime>(window: &WebviewWindow<R>) -> i32 {
    let scale = window.scale_factor().unwrap_or(1.0);
    (24.0 * scale).round() as i32
}

/// 记录新的托盘位置；没有新位置时返回上次记录的位置。
fn remember_tray<R: Runtime, M: Manager<R>>(manager: &M, tray: Option<Rect>) -> Option<Rect> {
    let state = manager.state::<WindowState>();
    let mut last = state.last_tray.lock().ok()?;
    if tray.is_some() {
        *last = tray;
    }
    *last
}

fn cancel_hide_animation<R: Runtime, M: Manager<R>>(manager: &M) {
    let state = manager.state::<WindowState>();
    state.animation.next();
    state.hiding.store(false, Ordering::SeqCst);
}

/// 隐藏窗口：先向下滑出屏幕，再按运行模式隐藏或释放。
fn hide<R: Runtime>(window: &tauri::Window<R>) {
    let state = window.state::<WindowState>();
    if state.hiding.swap(true, Ordering::SeqCst) {
        return;
    }
    // 也会取消进行中的滑入动画。
    let generation = state.animation.next();
    let window = window.clone();
    // 动画逐帧 sleep，放在独立线程，不阻塞事件循环。
    std::thread::spawn(move || {
        let origin = window.outer_position().ok();
        let completed = match (origin, window.current_monitor().ok().flatten()) {
            (Some(origin), Some(monitor)) => {
                let bottom = monitor.position().y + monitor.size().height as i32;
                let state = window.state::<WindowState>();
                animation::slide(
                    animation::EXIT,
                    fps(&window),
                    origin.y,
                    bottom,
                    || state.animation.is_current(generation),
                    |y| {
                        let _ = window.set_position(PhysicalPosition::new(origin.x, y));
                    },
                )
            }
            // 取不到位置时跳过动画，直接隐藏。
            _ => true,
        };
        if !completed {
            return;
        }
        let state = window.state::<WindowState>();
        if !state.animation.is_current(generation) {
            return;
        }
        let destroyed = finish_hide(&window);
        // 没有托盘位置可用于下次定位时，把窗口移回原位，避免下次从屏幕外出现。
        // 有托盘位置时不移动：下次打开会重新定位，也避免任何隐藏过渡在原位出现残影。
        let has_tray = state.last_tray.lock().ok().is_some_and(|t| t.is_some());
        if !destroyed
            && !has_tray
            && let Some(origin) = origin
        {
            let _ = window.set_position(origin);
        }
        state.hiding.store(false, Ordering::SeqCst);
    });
}

/// 按运行模式隐藏或释放窗口，返回窗口是否已销毁。
fn finish_hide<R: Runtime>(window: &tauri::Window<R>) -> bool {
    match policy(window) {
        WindowPolicy::Resident => {
            let _ = window.hide();
            false
        }
        WindowPolicy::Silent => {
            let _ = window.destroy();
            true
        }
        WindowPolicy::Smart => {
            let _ = window.hide();
            start_release_timer(window);
            false
        }
    }
}

/// 根据内容高度计算窗口高度（物理像素）：不低于最小高度，
/// 不超过屏幕高度的 4/5，也不超出工作区（留出与任务栏的间距）。
pub fn fitted_height(content: f64, scale: f64, screen_height: f64, work_area_height: f64) -> u32 {
    let max = (screen_height * MAX_SCREEN_RATIO).min(work_area_height - 2.0 * MARGIN * scale);
    let min = (MIN_HEIGHT * scale).min(max);
    (content * scale).clamp(min, max).round() as u32
}

/// 前端报告内容高度（逻辑像素）后调整窗口高度，并保持贴近托盘。
pub fn fit_height<R: Runtime>(window: &WebviewWindow<R>, content: f64) {
    if let Err(e) = try_fit_height(window, content) {
        eprintln!("[window] 调整高度失败：{e}");
    }
}

fn try_fit_height<R: Runtime>(window: &WebviewWindow<R>, content: f64) -> tauri::Result<()> {
    if !content.is_finite() || content <= 0.0 {
        return Ok(());
    }
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let height = fitted_height(
        content,
        monitor.scale_factor(),
        monitor.size().height.into(),
        monitor.work_area().size.height.into(),
    );
    let size = window.outer_size()?;
    if size.height == height {
        return Ok(());
    }
    window.set_size(PhysicalSize::new(size.width, height))?;

    // 可见时立即重新定位；隐藏时由下次显示负责定位。滑出动画进行中不打断。
    let state = window.state::<WindowState>();
    if window.is_visible()? && !state.hiding.load(Ordering::SeqCst) {
        let tray = state.last_tray.lock().ok().and_then(|t| *t);
        if let Some(tray) = tray {
            move_near_tray(window, tray)?;
        }
    }
    Ok(())
}

fn move_near_tray<R: Runtime>(window: &WebviewWindow<R>, tray: Rect) -> tauri::Result<()> {
    if let Some(target) = target_near_tray(window, tray)? {
        window.set_position(target)?;
    }
    Ok(())
}

/// 计算窗口贴近托盘时的左上角位置；取不到显示器信息时返回 `None`。
fn target_near_tray<R: Runtime>(
    window: &WebviewWindow<R>,
    tray: Rect,
) -> tauri::Result<Option<PhysicalPosition<i32>>> {
    let Some(monitor) = window
        .monitor_from_point(tray.x + tray.width / 2.0, tray.y + tray.height / 2.0)?
        .or(window.primary_monitor()?)
    else {
        return Ok(None);
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
    Ok(Some(PhysicalPosition::new(
        x.round() as i32,
        y.round() as i32,
    )))
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

    let hiding = app.state::<WindowState>().hiding.load(Ordering::SeqCst);
    match main_window(app) {
        // 滑出动画期间窗口仍可见，此时的点击视为“关闭”的延续，不重新打开。
        Some(_) if hiding => {}
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
    fn 高度随内容变化并有下限() {
        // 1080p、100% 缩放、工作区高 1032
        assert_eq!(fitted_height(300.0, 1.0, 1080.0, 1032.0), 300);
        assert_eq!(
            fitted_height(50.0, 1.0, 1080.0, 1032.0),
            160,
            "不低于最小高度"
        );
    }

    #[test]
    fn 高度不超过屏幕的五分之四() {
        assert_eq!(fitted_height(5000.0, 1.0, 1080.0, 1032.0), 864);
    }

    #[test]
    fn 高度按缩放比例换算为物理像素() {
        // 4K、200% 缩放：逻辑 300 → 物理 600
        assert_eq!(fitted_height(300.0, 2.0, 2160.0, 2064.0), 600);
        assert_eq!(fitted_height(5000.0, 2.0, 2160.0, 2064.0), 1728);
    }

    #[test]
    fn 工作区很矮时高度不超出工作区() {
        // 任务栏很高，工作区只剩 700：上限为 700 - 2×12
        assert_eq!(fitted_height(5000.0, 1.0, 1080.0, 700.0), 676);
    }

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
