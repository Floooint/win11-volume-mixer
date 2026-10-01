//! 主窗口：定位到托盘附近、显示 / 隐藏、失焦自动隐藏。
//!
//! 隐藏后的行为由设置项 [`WindowPolicy`] 决定：常驻（保留界面）、静默（立即释放界面）、
//! 智能（保留一段时间，超时未打开再释放）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::async_runtime::JoinHandle;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::animation::{self, Generation, Motion};
use crate::audio::{AudioService, AudioSnapshot, Command};
use crate::config::{
    Config, PopupDirection, PopupStyle, Settings, ThemeMode, WIDTH_RANGE, WindowPolicy,
};
use crate::timing::{OpenTiming, TimingMark};

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

/// 窗口固定方式，由标题栏的图钉按钮切换，只在本次运行中保持。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PinMode {
    /// 弹出面板：失焦自动隐藏，保持在最前。
    #[default]
    Normal,
    /// 定住：失焦不隐藏，可被其他窗口遮挡。
    Pinned,
    /// 定住并置顶：失焦不隐藏，始终在最前。
    PinnedOnTop,
}

impl PinMode {
    fn always_on_top(self) -> bool {
        self != Self::Pinned
    }

    fn hides_on_blur(self) -> bool {
        self == Self::Normal
    }
}

/// 窗口相关的全局状态。
#[derive(Default)]
pub struct WindowState {
    /// 最近一次因失焦而隐藏的时间。
    hidden_at: Mutex<Option<Instant>>,
    /// 按下托盘图标时窗口是否打开着，松开时据此决定打开还是关闭。
    open_at_press: Mutex<Option<bool>>,
    /// 正在创建的窗口：等前端首次渲染完成后再定位并显示，避免白屏闪烁。
    pending: Mutex<Option<Pending>>,
    /// 智能模式下的释放计时器；再次打开窗口时取消。
    release_timer: Mutex<Option<JoinHandle<()>>>,
    /// 窗口已创建、前端尚未首次渲染完成。此时打开要等 `ready`，否则会显示空白窗口。
    loading: AtomicBool,
    /// 光标是否在任务栏上（设置项 `prewarmOnHover` 开启时才跟踪）。
    over_taskbar: AtomicBool,
    /// 窗口是为预加载而创建（或保留）的，还没有显示过；打开后清除，之后按运行模式处理。
    prewarmed: AtomicBool,
    /// 光标移出任务栏后释放预加载窗口的计时器；回到任务栏或打开窗口时取消。
    prewarm_timer: Mutex<Option<JoinHandle<()>>>,
    /// 最近一次托盘图标位置，用于高度变化后重新定位，以及没有托盘位置的打开请求。
    last_tray: Mutex<Option<Rect>>,
    /// 滑出动画代数，打开窗口时递增以取消进行中的动画。
    animation: Generation,
    /// 是否正在播放滑出动画。
    hiding: AtomicBool,
    /// 是否正在播放滑入动画。此时高度变化不做动画，避免两个动画同时移动窗口。
    entering: AtomicBool,
    /// 滑入动画使用的窗口外框尺寸。滑入期间的高度变化只更新这里，由滑入动画逐帧应用；
    /// 滑入结束时在持有锁的情况下清除，之后的高度变化走正常的高度动画。
    enter_size: Mutex<Option<PhysicalSize<u32>>>,
    /// 为打开而新建窗口时的分段计时，前端报告内容就绪后输出并清除。
    open_timing: Mutex<Option<OpenTiming>>,
    /// 高度动画代数，新的高度变化会取消进行中的动画。
    resize: Generation,
    /// 设置页拖动宽度滑块时的预览宽度（逻辑像素），保存后清除。
    preview_width: Mutex<Option<u32>>,
    /// 首次创建窗口时的“硬件加速”设置。WebView2 的启动参数只在浏览器进程创建时生效，
    /// 且同一数据目录下参数必须一致，因此整个程序运行期间固定使用这个值，改设置后重启生效。
    hardware_acceleration: OnceLock<bool>,
    pin: Mutex<PinMode>,
    /// 首次运行时自动打开的窗口在回答“是否开机自启”之前不因失焦隐藏：
    /// 启动程序的窗口（如安装程序）随后可能夺回焦点，询问会一闪而过。
    hold_open: AtomicBool,
}

struct Pending {
    tray: Option<Rect>,
    requested_at: Instant,
}

/// 当前窗口宽度（逻辑像素）：正在预览时用预览值，否则用设置值。
fn logical_width<R: Runtime, M: Manager<R>>(manager: &M) -> f64 {
    let preview = manager
        .state::<WindowState>()
        .preview_width
        .lock()
        .ok()
        .and_then(|w| *w);
    let width = preview.unwrap_or_else(|| manager.state::<Config>().read(|s| s.window_width));
    f64::from(width.clamp(*WIDTH_RANGE.start(), *WIDTH_RANGE.end()))
}

fn policy<R: Runtime, M: Manager<R>>(manager: &M) -> WindowPolicy {
    manager.state::<Config>().read(|s| s.window_policy)
}

/// 动画帧率：用户设置优先，否则跟随窗口所在显示器的刷新率。
fn fps<R: Runtime>(window: &tauri::Window<R>) -> u32 {
    let configured = window.state::<Config>().read(|s| s.animation_fps);
    let refresh_rate = configured.is_none().then(|| {
        let monitor = window.current_monitor().ok().flatten();
        animation::display_refresh_rate(monitor.as_ref().and_then(|m| m.name()).map(String::as_str))
    });
    animation::effective_fps(configured, refresh_rate.flatten())
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
    let seconds = window.state::<Config>().read(|s| s.smart_release_seconds);
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

/// 光标移出任务栏后，预加载的窗口保留多久再释放。
const PREWARM_RELEASE: Duration = Duration::from_secs(5);

fn prewarm_enabled<R: Runtime, M: Manager<R>>(manager: &M) -> bool {
    manager.state::<Config>().read(|s| s.prewarm_on_hover)
}

fn cancel_prewarm_timer<R: Runtime, M: Manager<R>>(manager: &M) {
    let timer = state_of(manager)
        .prewarm_timer
        .lock()
        .ok()
        .and_then(|mut t| t.take());
    if let Some(timer) = timer {
        timer.abort();
    }
}

/// 光标进入或离开任务栏（含托盘区域）。只能在主线程调用：创建窗口必须在主线程上，
/// 且不能在窗口过程内（WebView 创建期间会处理消息），因此由 `run_on_main_thread` 转发过来。
///
/// 进入时若没有窗口，在后台创建（不显示），点击托盘图标时即可直接显示；
/// 离开 5 秒后仍未打开则释放。常驻模式和智能模式计时内窗口本来就在，不受影响。
pub fn taskbar_hover<R: Runtime>(app: &AppHandle<R>, over: bool) {
    let state = state_of(app);
    state.over_taskbar.store(over, Ordering::SeqCst);
    if !over {
        if state.prewarmed.load(Ordering::SeqCst) {
            start_prewarm_timer(app);
        }
        return;
    }
    cancel_prewarm_timer(app);
    if !prewarm_enabled(app) || main_window(app).is_some() {
        return;
    }
    match create(app) {
        Ok(_) => {
            state.prewarmed.store(true, Ordering::SeqCst);
            eprintln!("[window] 鼠标移到任务栏，预加载窗口");
        }
        Err(e) => eprintln!("[window] 预加载窗口失败：{e}"),
    }
}

fn start_prewarm_timer<R: Runtime>(app: &AppHandle<R>) {
    cancel_prewarm_timer(app);
    let handle = app.clone();
    let timer = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PREWARM_RELEASE).await;
        let state = state_of(&handle);
        // 期间可能已打开窗口或回到任务栏，到期时再确认一次。
        let waiting_to_show = state.pending.lock().ok().is_some_and(|p| p.is_some());
        if state.prewarmed.swap(false, Ordering::SeqCst)
            && !state.over_taskbar.load(Ordering::SeqCst)
            && !waiting_to_show
            && let Some(window) = main_window(&handle)
            && !window.is_visible().unwrap_or(true)
        {
            eprintln!("[window] 鼠标离开任务栏 5 秒，释放预加载的窗口");
            let _ = window.destroy();
        }
    });
    if let Ok(mut t) = state_of(app).prewarm_timer.lock() {
        *t = Some(timer);
    }
}

/// 设置项 `prewarmOnHover` 关闭后，释放还没显示过的预加载窗口（窗口隐藏时）。
pub fn stop_prewarm<R: Runtime>(app: &AppHandle<R>) {
    cancel_prewarm_timer(app);
    let state = state_of(app);
    state.over_taskbar.store(false, Ordering::SeqCst);
    if state.prewarmed.swap(false, Ordering::SeqCst)
        && policy(app) != WindowPolicy::Resident
        && let Some(window) = main_window(app)
        && !window.is_visible().unwrap_or(true)
    {
        let _ = window.destroy();
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

/// 窗口滑入 / 滑出所在的一侧：从这一侧进入，向这一侧离开。“自动”时为任务栏所在的边。
fn popup_edge(direction: PopupDirection, monitor: Rect, work_area: Rect) -> Edge {
    match direction {
        PopupDirection::Auto => taskbar_edge(monitor, work_area),
        PopupDirection::FromBottom => Edge::Bottom,
        PopupDirection::FromTop => Edge::Top,
        PopupDirection::FromLeft => Edge::Left,
        PopupDirection::FromRight => Edge::Right,
    }
}

/// 位于 `pos`、外框尺寸为 `size` 的窗口沿 `edge` 方向刚好完全移出显示器时的左上角位置。
fn offscreen_position(edge: Edge, pos: (i32, i32), size: (i32, i32), monitor: Rect) -> (i32, i32) {
    let (x, y) = pos;
    match edge {
        Edge::Bottom => (x, monitor.bottom().round() as i32),
        Edge::Top => (x, monitor.y.round() as i32 - size.1),
        Edge::Left => (monitor.x.round() as i32 - size.0, y),
        Edge::Right => (monitor.right().round() as i32, y),
    }
}

/// 滑入的起点：轻微滑入时从目标位置朝 `edge` 一侧偏移 `distance` 处开始，
/// 从屏幕边缘滑入时从屏幕外开始，无动画时就是目标位置。
fn enter_start(
    style: PopupStyle,
    edge: Edge,
    target: (i32, i32),
    size: (i32, i32),
    monitor: Rect,
    distance: i32,
) -> (i32, i32) {
    let (x, y) = target;
    match (style, edge) {
        (PopupStyle::None, _) => target,
        (PopupStyle::SlideFromEdge, _) => offscreen_position(edge, target, size, monitor),
        (PopupStyle::Slide, Edge::Bottom) => (x, y + distance),
        (PopupStyle::Slide, Edge::Top) => (x, y - distance),
        (PopupStyle::Slide, Edge::Left) => (x - distance, y),
        (PopupStyle::Slide, Edge::Right) => (x + distance, y),
    }
}

/// 按设置得出的弹出动画：所在的一侧，以及按样式和速度调整后的进入 / 退出动画。
struct Popup {
    edge: Edge,
    style: PopupStyle,
    enter: Motion,
    exit: Motion,
}

fn popup<R: Runtime, M: Manager<R>>(manager: &M, monitor: &tauri::Monitor) -> Popup {
    let (direction, style, speed) = manager
        .state::<Config>()
        .read(|s| (s.popup_direction, s.popup_style, s.popup_speed));
    let (screen, work_area) = monitor_rects(monitor);
    let enter = match style {
        PopupStyle::SlideFromEdge => animation::ENTER_FROM_EDGE,
        _ => animation::ENTER,
    };
    let factor = match style {
        PopupStyle::None => 0.0,
        _ => speed.duration_factor(),
    };
    Popup {
        edge: popup_edge(direction, screen, work_area),
        style,
        enter: enter.scaled(factor),
        exit: animation::EXIT.scaled(factor),
    }
}

/// 显示器区域与工作区（物理像素）。
fn monitor_rects(monitor: &tauri::Monitor) -> (Rect, Rect) {
    let to_rect = |pos: PhysicalPosition<i32>, size: PhysicalSize<u32>| Rect {
        x: pos.x.into(),
        y: pos.y.into(),
        width: size.width.into(),
        height: size.height.into(),
    };
    let area = monitor.work_area();
    (
        to_rect(*monitor.position(), *monitor.size()),
        to_rect(area.position, area.size),
    )
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

/// 设置中的深浅色对应的窗口主题。`None` 跟随系统。
/// 窗口主题同时决定 Mica 背景的深浅和网页的 `prefers-color-scheme`。
fn window_theme(mode: ThemeMode) -> Option<tauri::Theme> {
    match mode {
        ThemeMode::System => None,
        ThemeMode::Light => Some(tauri::Theme::Light),
        ThemeMode::Dark => Some(tauri::Theme::Dark),
    }
}

/// 设置变化后立即应用深浅色。
pub fn apply_theme<R: Runtime>(window: &WebviewWindow<R>) {
    let mode = window.state::<Config>().read(|s| s.theme);
    if let Err(e) = window.set_theme(window_theme(mode)) {
        eprintln!("[window] 切换深浅色失败：{e}");
    }
}

/// WebView2 启动参数。设置后会替换 wry 的默认参数，因此要把默认参数一并带上。
fn browser_args(hardware_acceleration: bool) -> String {
    // wry 的默认参数：去掉选中文字时的迷你菜单和 SmartScreen。
    let mut args = String::from("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection");
    if !hardware_acceleration {
        // GPU 进程从约 82 MB 降到约 14 MB，Mica 背景和动画不受影响。
        args.push_str(" --disable-gpu");
    }
    args
}

/// 本次运行使用的 WebView2 启动参数。所有窗口必须相同，才能共用同一个浏览器进程；
/// 硬件加速设置在首次创建窗口时读取，修改后重启程序才生效。
pub(crate) fn browser_args_for<R: Runtime>(app: &AppHandle<R>) -> String {
    let hardware_acceleration = *app
        .state::<WindowState>()
        .hardware_acceleration
        .get_or_init(|| app.state::<Config>().read(|s| s.hardware_acceleration));
    browser_args(hardware_acceleration)
}

/// 设置中的深浅色对应的窗口主题，新建窗口时使用。
pub(crate) fn theme_for<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::Theme> {
    window_theme(app.state::<Config>().read(|s| s.theme))
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
    mark_open(app, "开始创建窗口");
    state_of(app).loading.store(true, Ordering::SeqCst);
    let window = WebviewWindowBuilder::from_config(app, config)?
        .additional_browser_args(&browser_args_for(app))
        .theme(theme_for(app))
        .initialization_script(initial_state_script(app))
        .build()?;
    // 包含启动 WebView2 浏览器进程和创建控件。
    mark_open(app, "WebView 创建完成");
    disable_system_transitions(&window);
    // 窗口被释放后重建时，恢复当前的固定方式（配置中默认置顶）。
    let _ = window.set_always_on_top(pin_mode(app).always_on_top());
    let handle = app.clone();
    let _ = window.with_webview(move |webview| {
        crate::context_menu::install(&handle, &webview.controller(), &webview.environment())
    });
    Ok(window)
}

/// 等待音频快照的上限。音频线程通常 1 ms 内回复；忙时不拖慢窗口创建，改由前端自行读取。
const SNAPSHOT_TIMEOUT: Duration = Duration::from_millis(50);

/// 页面脚本运行前注入的初始数据：音频快照和设置。前端首次渲染就能显示完整列表，
/// 不必先显示骨架屏、等读取完成再换成列表（窗口高度也因此不再变化）。见 `src/lib/initial-state.ts`。
/// 快照在创建窗口时读取，到前端订阅事件之间的变化由前端订阅后重新读取一次补上。
fn initial_state_script<R: Runtime>(app: &AppHandle<R>) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct InitialState {
        snapshot: Option<AudioSnapshot>,
        settings: Settings,
        defaults: Settings,
    }
    // 主线程上同步等待；在异步运行时的线程上不能 block_on，此时跳过快照。
    let snapshot = tokio::runtime::Handle::try_current()
        .is_err()
        .then(|| {
            let audio = app.state::<AudioService>();
            // 计时器必须在运行时内创建，因此放进 async 块，不能作为 block_on 的参数直接构造。
            tauri::async_runtime::block_on(async {
                tokio::time::timeout(SNAPSHOT_TIMEOUT, audio.request(Command::Snapshot)).await
            })
        })
        .and_then(|result| result.ok()?.ok())
        // 没有设备的快照（如程序刚启动、音频线程还没打开设备）不注入，免得先显示“没有设备”。
        .filter(|snapshot| snapshot.device.is_some());
    let state = InitialState {
        snapshot,
        settings: app.state::<Config>().get(),
        defaults: Settings::default(),
    };
    match serde_json::to_string(&state) {
        Ok(json) => format!("window.__INITIAL_STATE__ = {json};"),
        Err(e) => {
            eprintln!("[window] 序列化初始数据失败：{e}");
            String::new()
        }
    }
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
/// `show` 为 `true` 时（首次运行）创建后立即在托盘位置 `tray` 附近显示。
pub fn init<R: Runtime>(app: &AppHandle<R>, show: bool, tray: Option<Rect>) -> tauri::Result<()> {
    let policy = policy(app);
    eprintln!("[window] 运行模式：{policy:?}");
    if show {
        app.state::<WindowState>()
            .hold_open
            .store(true, Ordering::SeqCst);
        // 等前端渲染完成后由 `ready` 显示。
        if let Ok(mut pending) = app.state::<WindowState>().pending.lock() {
            *pending = Some(Pending {
                tray,
                requested_at: Instant::now(),
            });
        }
        create(app)?;
    } else if policy != WindowPolicy::Silent {
        create(app)?;
    }
    Ok(())
}

/// 在托盘图标附近显示窗口；`tray` 为 `None` 时（取不到托盘位置）使用上次的托盘位置。
pub fn show<R: Runtime>(app: &AppHandle<R>, tray: Option<Rect>) {
    let requested_at = Instant::now();
    cancel_release_timer(app);
    cancel_prewarm_timer(app);
    cancel_hide_animation(app);
    // 打开后不再算作预加载窗口，之后按运行模式处理。
    let prewarmed = state_of(app).prewarmed.swap(false, Ordering::SeqCst);
    match main_window(app) {
        Some(window) => {
            // 窗口正在创建、尚未渲染完成时，等 `ready` 统一显示。
            let state = app.state::<WindowState>();
            if let Ok(mut pending) = state.pending.lock() {
                if let Some(pending) = pending.as_mut() {
                    pending.tray = tray.or(pending.tray);
                    return;
                }
                // 预加载的窗口还在加载（鼠标移到任务栏后很快就点击了）。
                if state.loading.load(Ordering::SeqCst) {
                    *pending = Some(Pending { tray, requested_at });
                    if let Ok(mut timing) = state.open_timing.lock() {
                        *timing = Some(OpenTiming::new(requested_at));
                    }
                    return;
                }
            }
            if prewarmed {
                eprintln!("[window] 使用预加载的窗口");
            }
            present(&window, tray);
            eprintln!(
                "[window] 打开耗时 {:?}（窗口已存在）",
                requested_at.elapsed()
            );
        }
        None => {
            if let Ok(mut timing) = state_of(app).open_timing.lock() {
                *timing = Some(OpenTiming::new(requested_at));
            }
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
    state_of(app).loading.store(false, Ordering::SeqCst);
    let pending = app
        .state::<WindowState>()
        .pending
        .lock()
        .ok()
        .and_then(|mut p| p.take());
    if let (Some(pending), Some(window)) = (pending, main_window(app)) {
        mark_open(app, "窗口开始显示（滑入）");
        present(&window, pending.tray);
        eprintln!(
            "[window] 打开耗时 {:?}（新建窗口）",
            pending.requested_at.elapsed()
        );
    }
}

fn state_of<R: Runtime, M: Manager<R>>(manager: &M) -> tauri::State<'_, WindowState> {
    manager.state::<WindowState>()
}

/// 正在为打开而新建窗口时，记下一个时间点。
fn mark_open<R: Runtime, M: Manager<R>>(manager: &M, name: &str) {
    if let Ok(mut timing) = state_of(manager).open_timing.lock()
        && let Some(timing) = timing.as_mut()
    {
        timing.mark(name);
    }
}

/// 前端报告新建窗口内容就绪：合并前端的时间点，输出分段耗时。
/// 不是为打开而新建的窗口（如启动时预先创建）没有计时，忽略。
pub fn report_open_timing<R: Runtime>(app: &AppHandle<R>, marks: &[TimingMark]) {
    let timing = state_of(app)
        .open_timing
        .lock()
        .ok()
        .and_then(|mut t| t.take());
    if let Some(mut timing) = timing {
        for mark in marks {
            timing.mark_epoch(mark);
        }
        eprintln!("{}", timing.summary());
    }
}

fn present<R: Runtime>(window: &WebviewWindow<R>, tray: Option<Rect>) {
    let tray = remember_tray(window, tray);
    let tray_monitor = match tray.map(|tray| tray_monitor(window, tray)) {
        Some(Ok(monitor)) => monitor,
        Some(Err(e)) => {
            eprintln!("[window] 定位失败：{e}");
            None
        }
        None => None,
    };
    // 外框尺寸为 `size` 时贴近托盘的位置。滑入期间高度可能变化，每帧按最新尺寸重新计算。
    let layout = tray.zip(tray_monitor.as_ref()).map(|(tray, monitor)| {
        let (screen, work_area) = monitor_rects(monitor);
        (tray, screen, work_area, monitor.scale_factor())
    });
    let place = move |size: PhysicalSize<u32>| {
        layout.map(|(tray, screen, work_area, scale)| {
            let (x, y) = position_near_tray(
                tray,
                size.width.into(),
                size.height.into(),
                screen,
                work_area,
                scale,
            );
            PhysicalPosition::new(x.round() as i32, y.round() as i32)
        })
    };
    let size = window.outer_size().unwrap_or_default();
    // 用计算出的目标位置，而不是移动后再读回：非主线程上 set_position 是异步的，读回可能是旧值。
    let (Some(target), Ok(hwnd)) = (
        place(size).or_else(|| window.outer_position().ok()),
        window.hwnd(),
    ) else {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    };
    let hwnd = hwnd.0 as isize;
    let (width, height) = (size.width as i32, size.height as i32);
    let monitor = tray_monitor.or_else(|| window.current_monitor().ok().flatten());
    let (start, enter) = match &monitor {
        Some(monitor) => {
            let popup = popup(window, monitor);
            let start = enter_start(
                popup.style,
                popup.edge,
                (target.x, target.y),
                (width, height),
                monitor_rects(monitor).0,
                slide_distance(monitor.scale_factor()),
            );
            (start, popup.enter)
        }
        None => ((target.x, target.y), animation::ENTER.instant()),
    };

    // 先放到滑入的起点再显示，然后滑到目标位置。
    let _ = window.set_position(PhysicalPosition::new(start.0, start.1));
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();

    // 滑入动画只移动相对目标位置的偏移，目标位置本身随尺寸变化。
    let offset = (start.0 - target.x, start.1 - target.y);
    let bounds = move |size: PhysicalSize<u32>, progress: f64| {
        let base = place(size).unwrap_or(target);
        set_bounds(
            hwnd,
            base.x + animation::lerp(offset.0, 0, progress),
            base.y + animation::lerp(offset.1, 0, progress),
            size.width as i32,
            size.height as i32,
        );
    };

    let state = window.state::<WindowState>();
    let generation = state.animation.next();
    if let Ok(mut enter_size) = state.enter_size.lock() {
        *enter_size = Some(size);
    }
    state.entering.store(true, Ordering::SeqCst);
    let window = window.clone();
    std::thread::spawn(move || {
        let state = window.state::<WindowState>();
        let current_size = || {
            state
                .enter_size
                .lock()
                .ok()
                .and_then(|s| *s)
                .unwrap_or(size)
        };
        let completed = animation::run(
            enter,
            fps(&window.as_ref().window()),
            || state.animation.is_current(generation),
            |progress| bounds(current_size(), progress),
        );
        // 被新的动画取消时不修正位置，交给新动画处理。
        if !completed {
            return;
        }
        // 持有锁直到结束：此后的高度变化不再交给滑入动画，走正常的高度动画，不会丢失。
        let Ok(mut enter_size) = state.enter_size.lock() else {
            return;
        };
        if state.animation.is_current(generation) {
            bounds(enter_size.take().unwrap_or(size), 1.0);
            state.entering.store(false, Ordering::SeqCst);
        }
    });
}

/// 轻微滑入的距离（物理像素）：一小段，避免窗口从屏幕边缘整段飞入。
fn slide_distance(scale: f64) -> i32 {
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
    state.entering.store(false, Ordering::SeqCst);
}

/// 隐藏窗口：先向弹出方向所在的一侧滑出屏幕，再按运行模式隐藏或释放。
fn hide<R: Runtime>(window: &tauri::Window<R>) {
    let state = window.state::<WindowState>();
    if state.hiding.swap(true, Ordering::SeqCst) {
        return;
    }
    // 详情浮窗随主窗口一起消失，并释放它的内存。
    crate::details::destroy(window);
    // 也会取消进行中的滑入动画和高度动画。
    let generation = state.animation.next();
    state.entering.store(false, Ordering::SeqCst);
    state.resize.next();
    let window = window.clone();
    // 动画逐帧 sleep，放在独立线程，不阻塞事件循环。
    std::thread::spawn(move || {
        let origin = window.outer_position().ok();
        let monitor = window.current_monitor().ok().flatten();
        let size = window.outer_size().ok();
        let completed = match (origin, monitor, size) {
            (Some(origin), Some(monitor), Some(size)) => {
                let popup = popup(&window, &monitor);
                let to = offscreen_position(
                    popup.edge,
                    (origin.x, origin.y),
                    (size.width as i32, size.height as i32),
                    monitor_rects(&monitor).0,
                );
                let state = window.state::<WindowState>();
                animation::slide(
                    popup.exit,
                    fps(&window),
                    (origin.x, origin.y),
                    to,
                    || state.animation.is_current(generation),
                    |x, y| {
                        let _ = window.set_position(PhysicalPosition::new(x, y));
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
        // 光标还在任务栏上（如点击托盘图标关闭）时可能马上再打开：先保留为预加载窗口，
        // 移出任务栏 5 秒后再释放。
        WindowPolicy::Silent
            if prewarm_enabled(window) && state_of(window).over_taskbar.load(Ordering::SeqCst) =>
        {
            let _ = window.hide();
            state_of(window).prewarmed.store(true, Ordering::SeqCst);
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
    // 向上取整：窗口哪怕比内容矮不到 1 px，滚动区也会出现滚动条。
    (content * scale).clamp(min, max).ceil() as u32
}

/// 前端报告内容高度（逻辑像素）后调整窗口高度，并保持贴近托盘。
/// 窗口可见时以动画过渡，返回动画时长（毫秒），立即完成时返回 0。
pub fn fit_height<R: Runtime>(window: &WebviewWindow<R>, content: f64) -> u32 {
    match try_fit_height(window, content) {
        Ok(Some(duration)) => duration.as_millis() as u32,
        Ok(None) => 0,
        Err(e) => {
            eprintln!("[window] 调整高度失败：{e}");
            0
        }
    }
}

fn try_fit_height<R: Runtime>(
    window: &WebviewWindow<R>,
    content: f64,
) -> tauri::Result<Option<Duration>> {
    if !content.is_finite() || content <= 0.0 {
        return Ok(None);
    }
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(None);
    };
    let height = fitted_height(
        content,
        monitor.scale_factor(),
        monitor.size().height.into(),
        monitor.work_area().size.height.into(),
    );
    // `set_size` 设置的是内容区尺寸，而 `outer_size` 含边框。若用读回的外框宽度去设置，
    // 每次调整都会多出边框宽度，窗口越来越宽。因此宽度始终使用配置中的固定值。
    let width = (logical_width(window) * monitor.scale_factor()).round() as u32;
    let size = window.inner_size()?;
    let state = window.state::<WindowState>();
    // 滑入期间交给滑入动画：它每帧按最新尺寸重新贴近托盘，位置和尺寸一次设置。
    // 若在这里只改尺寸，窗口会以左上角为准向下伸出（新建窗口时列表稍后才加载，高度会变大），
    // 底边一直伸到屏幕底部，滑入结束后才跳回任务栏上方。
    if let Ok(mut enter_size) = state.enter_size.lock()
        && enter_size.is_some()
        && state.entering.load(Ordering::SeqCst)
    {
        let outer = window.outer_size()?;
        *enter_size = Some(PhysicalSize::new(
            width + outer.width.saturating_sub(size.width),
            height + outer.height.saturating_sub(size.height),
        ));
        return Ok(None);
    }
    if size.height == height && size.width == width {
        return Ok(None);
    }

    // 窗口高度变化时应用行会移动，详情浮窗与应用行对不齐，收起它。
    crate::details::hide(window);

    let tray = state.last_tray.lock().ok().and_then(|t| *t);
    // 滑出动画进行中不打断：只调整尺寸，隐藏后由下次显示负责定位。
    let visible = window.is_visible()?
        && !state.hiding.load(Ordering::SeqCst)
        && !state.entering.load(Ordering::SeqCst);
    if visible && let Some(tray) = tray {
        return animate_height(window, tray, PhysicalSize::new(width, height));
    }

    state.resize.next();
    window.set_size(PhysicalSize::new(width, height))?;
    Ok(None)
}

/// 预览宽度（设置页拖动滑块时），`None` 表示结束预览、恢复为设置值。立即生效，不写入设置。
pub fn preview_width<R: Runtime>(window: &WebviewWindow<R>, width: Option<u32>) {
    if let Ok(mut preview) = window.state::<WindowState>().preview_width.lock() {
        *preview = width;
    }
    apply_width(window);
}

/// 按当前宽度（预览值或设置值）调整窗口，高度不变，并保持贴近托盘。
pub fn apply_width<R: Runtime>(window: &WebviewWindow<R>) {
    if let Err(e) = try_apply_width(window) {
        eprintln!("[window] 调整宽度失败：{e}");
    }
}

fn try_apply_width<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    let inner = window.inner_size()?;
    let width = (logical_width(window) * window.scale_factor()?).round() as u32;
    if inner.width == width {
        return Ok(());
    }
    let state = window.state::<WindowState>();
    // 取消进行中的高度动画，它按旧宽度逐帧设置尺寸。
    state.resize.next();
    let tray = state.last_tray.lock().ok().and_then(|t| *t);
    let visible = window.is_visible()? && !state.hiding.load(Ordering::SeqCst);
    let new_inner = PhysicalSize::new(width, inner.height);
    if visible && let Some(tray) = tray {
        let outer = window.outer_size()?;
        let to_size = PhysicalSize::new(
            width + outer.width.saturating_sub(inner.width),
            outer.height,
        );
        if let Some(pos) = target_near_tray(window, tray, to_size)? {
            // 位置和尺寸一次设置，拖动滑块时窗口不会左右抖动。
            set_bounds(
                window.hwnd()?.0 as isize,
                pos.x,
                pos.y,
                to_size.width as i32,
                to_size.height as i32,
            );
            return Ok(());
        }
    }
    window.set_size(new_inner)
}

/// 以动画把窗口内容区调整到 `inner`，同时移动窗口保持贴近托盘（任务栏在底部时底边不动）。
/// 每帧用一次 `SetWindowPos` 同时设置位置和尺寸，避免先改尺寸再移动造成的跳动。
fn animate_height<R: Runtime>(
    window: &WebviewWindow<R>,
    tray: Rect,
    inner: PhysicalSize<u32>,
) -> tauri::Result<Option<Duration>> {
    let from_size = window.outer_size()?;
    let from_pos = window.outer_position()?;
    let current_inner = window.inner_size()?;
    let border_w = from_size.width.saturating_sub(current_inner.width);
    let border_h = from_size.height.saturating_sub(current_inner.height);
    let to_size = PhysicalSize::new(inner.width + border_w, inner.height + border_h);
    let Some(to_pos) = target_near_tray(window, tray, to_size)? else {
        window.set_size(inner)?;
        return Ok(None);
    };
    let hwnd = window.hwnd()?.0 as isize;

    let state = window.state::<WindowState>();
    let generation = state.resize.next();
    let fps = fps(&window.as_ref().window());
    let window = window.clone();
    std::thread::spawn(move || {
        let state = window.state::<WindowState>();
        animation::run(
            animation::RESIZE,
            fps,
            || state.resize.is_current(generation) && !state.hiding.load(Ordering::SeqCst),
            |p| {
                set_bounds(
                    hwnd,
                    animation::lerp(from_pos.x, to_pos.x, p),
                    animation::lerp(from_pos.y, to_pos.y, p),
                    animation::lerp(from_size.width as i32, to_size.width as i32, p),
                    animation::lerp(from_size.height as i32, to_size.height as i32, p),
                );
            },
        );
    });
    Ok(Some(animation::RESIZE.duration))
}

/// 同时设置窗口外框的位置和尺寸（物理像素）。异步投递给窗口所在线程，不等待它处理。
fn set_bounds(hwnd: isize, x: i32, y: i32, width: i32, height: i32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    };
    // SAFETY: hwnd 来自存活的主窗口；窗口销毁后调用只会返回错误。
    let result = unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        )
    };
    if let Err(e) = result {
        eprintln!("[window] 调整窗口失败：{e}");
    }
}

/// 托盘图标所在的显示器，取不到时为主显示器。
fn tray_monitor<R: Runtime>(
    window: &WebviewWindow<R>,
    tray: Rect,
) -> tauri::Result<Option<tauri::Monitor>> {
    Ok(window
        .monitor_from_point(tray.x + tray.width / 2.0, tray.y + tray.height / 2.0)?
        .or(window.primary_monitor()?))
}

/// 计算外框尺寸为 `size` 的窗口贴近托盘时的左上角位置；取不到显示器信息时返回 `None`。
fn target_near_tray<R: Runtime>(
    window: &WebviewWindow<R>,
    tray: Rect,
    size: PhysicalSize<u32>,
) -> tauri::Result<Option<PhysicalPosition<i32>>> {
    let Some(monitor) = tray_monitor(window, tray)? else {
        return Ok(None);
    };
    let (screen, work_area) = monitor_rects(&monitor);
    let (x, y) = position_near_tray(
        tray,
        size.width.into(),
        size.height.into(),
        screen,
        work_area,
        monitor.scale_factor(),
    );
    Ok(Some(PhysicalPosition::new(
        x.round() as i32,
        y.round() as i32,
    )))
}

pub fn pin_mode<R: Runtime, M: Manager<R>>(manager: &M) -> PinMode {
    manager
        .state::<WindowState>()
        .pin
        .lock()
        .map(|p| *p)
        .unwrap_or_default()
}

pub fn set_pin_mode<R: Runtime>(window: &WebviewWindow<R>, mode: PinMode) {
    if let Ok(mut pin) = window.state::<WindowState>().pin.lock() {
        *pin = mode;
    }
    if let Err(e) = window.set_always_on_top(mode.always_on_top()) {
        eprintln!("[window] 设置置顶失败：{e}");
    }
}

/// 已回答首次运行的询问，窗口恢复失焦自动隐藏。
pub fn release_hold<R: Runtime, M: Manager<R>>(manager: &M) {
    manager
        .state::<WindowState>()
        .hold_open
        .store(false, Ordering::SeqCst);
}

/// 托盘左键按下：记下窗口此时是否打开着。按下时任务栏获得焦点，窗口随即失焦隐藏，
/// 松开时再看窗口状态就分不清这次点击是要关闭还是打开。
pub fn tray_pressed<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<WindowState>();
    let visible = main_window(app).is_some_and(|w| w.is_visible().unwrap_or(false))
        && !state.hiding.load(Ordering::SeqCst);
    // 失焦事件可能先于按下事件到达：刚因失焦开始隐藏的，也算按下时打开着。
    let just_blurred = state
        .hidden_at
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if let Ok(mut open) = state.open_at_press.lock() {
        *open = Some(visible || just_blurred);
    }
}

/// 托盘左键点击（松开）：按下时窗口打开着则关闭，否则在托盘附近显示。
pub fn toggle<R: Runtime>(app: &AppHandle<R>, tray: Rect) {
    let open_at_press = app
        .state::<WindowState>()
        .open_at_press
        .lock()
        .ok()
        .and_then(|mut t| t.take());
    if let Some(open) = open_at_press {
        let hiding = app.state::<WindowState>().hiding.load(Ordering::SeqCst);
        match main_window(app) {
            // 按下时已因失焦开始隐藏的，不需要再处理。
            Some(window) if open => {
                if window.is_visible().unwrap_or(false) && !hiding {
                    hide(&window.as_ref().window());
                }
            }
            _ if !open => show(app, Some(tray)),
            _ => {}
        }
        return;
    }
    // 没有收到按下事件时，按失焦隐藏的时间判断。
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
///
/// 主窗口和详情浮窗算作一个整体：点击浮窗（WebView2 会激活它）或从浮窗点回主窗口时，
/// 焦点只是在两者之间切换，不算失焦。焦点离开两者时收起浮窗，主窗口按固定模式决定是否隐藏。
pub fn handle_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    let label = window.label();
    if label != MAIN && label != crate::details::LABEL {
        return;
    }
    match event {
        WindowEvent::Focused(false) if !foreground_is_ours(window) => {
            crate::details::hide(window);
            hide_on_blur(window.app_handle());
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if label == MAIN {
                hide(window);
            } else {
                // 浮窗有焦点时按 Alt+F4：只隐藏，保留窗口并清除显示状态。
                crate::details::hide(window);
            }
        }
        _ => {}
    }
}

/// 前台窗口是主窗口或详情浮窗。失焦事件到达时前台窗口已经切换。
fn foreground_is_ours<R: Runtime, M: Manager<R>>(manager: &M) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // SAFETY: 只读取前台窗口句柄。
    let foreground = unsafe { GetForegroundWindow() };
    [MAIN, crate::details::LABEL].iter().any(|label| {
        manager
            .get_webview_window(label)
            .and_then(|w| w.hwnd().ok())
            .is_some_and(|hwnd| hwnd.0 == foreground.0)
    })
}

/// 焦点离开主窗口和详情浮窗：按固定模式隐藏主窗口。
fn hide_on_blur<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = main_window(app) else {
        return;
    };
    let state = app.state::<WindowState>();
    // 新建窗口在渲染完成前不可见，此时的失焦事件忽略。
    if !window.is_visible().unwrap_or(false)
        || !pin_mode(app).hides_on_blur()
        || state.hold_open.load(Ordering::SeqCst)
    {
        return;
    }
    // 记下隐藏时间：点击托盘图标导致的失焦隐藏后，同一次点击不再重新打开窗口。
    if let Ok(mut t) = state.hidden_at.lock() {
        *t = Some(Instant::now());
    }
    hide(&window.as_ref().window());
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
    fn 固定方式决定失焦隐藏和置顶() {
        assert!(PinMode::Normal.hides_on_blur() && PinMode::Normal.always_on_top());
        assert!(!PinMode::Pinned.hides_on_blur() && !PinMode::Pinned.always_on_top());
        assert!(!PinMode::PinnedOnTop.hides_on_blur() && PinMode::PinnedOnTop.always_on_top());
    }

    #[test]
    fn 关闭硬件加速时追加禁用_gpu_参数且保留默认参数() {
        let default = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
        assert_eq!(browser_args(true), default);
        assert_eq!(browser_args(false), format!("{default} --disable-gpu"));
    }

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
    fn 自动方向跟随任务栏所在的边() {
        let cases = [
            (rect(0.0, 0.0, 1920.0, 1032.0), Edge::Bottom),
            (rect(0.0, 48.0, 1920.0, 1032.0), Edge::Top),
            (rect(64.0, 0.0, 1856.0, 1080.0), Edge::Left),
            (rect(0.0, 0.0, 1856.0, 1080.0), Edge::Right),
        ];
        for (work, edge) in cases {
            assert_eq!(popup_edge(PopupDirection::Auto, MONITOR, work), edge);
        }
    }

    #[test]
    fn 指定方向时不看任务栏位置() {
        let top_taskbar = rect(0.0, 48.0, 1920.0, 1032.0);
        assert_eq!(
            popup_edge(PopupDirection::FromBottom, MONITOR, top_taskbar),
            Edge::Bottom
        );
        assert_eq!(
            popup_edge(PopupDirection::FromRight, MONITOR, top_taskbar),
            Edge::Right
        );
    }

    #[test]
    fn 滑出时刚好完全离开屏幕() {
        let (pos, size) = ((1500, 60), (380, 520));
        assert_eq!(
            offscreen_position(Edge::Bottom, pos, size, MONITOR),
            (1500, 1080)
        );
        assert_eq!(
            offscreen_position(Edge::Top, pos, size, MONITOR),
            (1500, -520)
        );
        assert_eq!(
            offscreen_position(Edge::Left, pos, size, MONITOR),
            (-380, 60)
        );
        assert_eq!(
            offscreen_position(Edge::Right, pos, size, MONITOR),
            (1920, 60)
        );
    }

    #[test]
    fn 副显示器上滑出到该显示器之外() {
        let monitor = rect(1920.0, -200.0, 1920.0, 1080.0);
        let (pos, size) = ((2000, 0), (380, 520));
        assert_eq!(
            offscreen_position(Edge::Top, pos, size, monitor),
            (2000, -720)
        );
        assert_eq!(
            offscreen_position(Edge::Left, pos, size, monitor),
            (1540, 0)
        );
    }

    #[test]
    fn 轻微滑入从任务栏一侧偏移一小段() {
        let start =
            |edge| enter_start(PopupStyle::Slide, edge, (100, 200), (380, 520), MONITOR, 24);
        assert_eq!(start(Edge::Bottom), (100, 224));
        assert_eq!(start(Edge::Top), (100, 176), "任务栏在顶部时从上方滑入");
        assert_eq!(start(Edge::Left), (76, 200));
        assert_eq!(start(Edge::Right), (124, 200));
    }

    #[test]
    fn 从屏幕边缘滑入从屏幕外开始() {
        let start = enter_start(
            PopupStyle::SlideFromEdge,
            Edge::Top,
            (100, 60),
            (380, 520),
            MONITOR,
            24,
        );
        assert_eq!(start, (100, -520));
    }

    #[test]
    fn 无动画时直接在目标位置显示() {
        let start = enter_start(
            PopupStyle::None,
            Edge::Bottom,
            (100, 200),
            (380, 520),
            MONITOR,
            24,
        );
        assert_eq!(start, (100, 200));
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
