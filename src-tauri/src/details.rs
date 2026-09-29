//! 应用详情浮窗：鼠标在应用上停留 1 秒后，在主窗口旁（优先左侧）显示应用名、进程名、路径等。
//!
//! 单独的无边框小窗口（与主窗口同样的 Mica 背景），带 `WS_EX_NOACTIVATE`，显示时不抢焦点，
//! 主窗口不会因失焦而隐藏。首次需要时才创建，主窗口隐藏时销毁，平时不占内存。
//!
//! 流程：主窗口前端调用 [`show`] 记下要显示的内容和应用行的位置 → 浮窗前端渲染后
//! 调用 [`ready`] 报告内容高度 → 后端据此定位并显示。内容更新通过 [`AppDetailsEvent`] 推送。
//!
//! 浮窗中的文字可以选择、右键复制，路径可点击打开所在文件夹，所以鼠标要能移到浮窗上：
//! 离开应用行后稍等片刻才隐藏（[`hide_soon`]），期间移入浮窗则保持显示，移出浮窗后再隐藏。
//! 鼠标在浮窗上时主窗口也不因失焦隐藏（如点击路径打开了资源管理器）。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow, WebviewWindowBuilder};
use tauri_specta::Event;

use crate::animation::Generation;
use crate::audio::AppAudio;
use crate::window::{MAIN, Rect};

pub const LABEL: &str = "details";

/// 浮窗宽度与离主窗口的间距（逻辑像素）。
const WIDTH: f64 = 300.0;
const GAP: f64 = 8.0;
/// 鼠标离开应用行或浮窗后等多久隐藏：留出把鼠标移到浮窗上的时间。
/// 与前端 `src/lib/hover-details.ts` 的 `HIDE_MS` 一致。
const HIDE_DELAY: Duration = Duration::from_millis(1000);

/// 浮窗显示的内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppDetails {
    pub app: AppAudio,
    /// 用户起的名称。
    pub alias: Option<String>,
}

/// 要显示的内容变化时推送给浮窗。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "details://show")]
pub struct AppDetailsEvent(pub AppDetails);

/// 当前的显示请求。
struct Request {
    details: AppDetails,
    /// 应用行的上边缘，相对主窗口内容区（逻辑像素）。
    anchor_top: f64,
}

#[derive(Default)]
pub struct DetailsState {
    request: Mutex<Option<Request>>,
    /// 鼠标在浮窗上。
    hovered: AtomicBool,
    /// 延迟隐藏的代数：显示、立即隐藏或再次延迟隐藏时递增，取消之前的延迟隐藏。
    hide: Generation,
}

/// 显示某个应用的详情。`anchor_top` 为应用行的上边缘，相对主窗口内容区（逻辑像素）。
pub fn show<R: Runtime>(app: &AppHandle<R>, details: AppDetails, anchor_top: f64) {
    let state = app.state::<DetailsState>();
    state.hide.next();
    if let Ok(mut request) = state.request.lock() {
        *request = Some(Request {
            details: details.clone(),
            anchor_top,
        });
    }
    match app.get_webview_window(LABEL) {
        // 已有浮窗：推送新内容，浮窗重新渲染后调用 `ready`。
        Some(_) => {
            if let Err(e) = AppDetailsEvent(details).emit_to(app, LABEL) {
                eprintln!("[details] 推送详情失败：{e}");
            }
        }
        None => {
            if let Err(e) = create(app) {
                eprintln!("[details] 创建浮窗失败：{e}");
            }
        }
    }
}

/// 浮窗前端首次加载时读取要显示的内容。
pub fn current<R: Runtime>(app: &AppHandle<R>) -> Option<AppDetails> {
    let state = app.state::<DetailsState>();
    let request = state.request.lock().ok()?;
    request.as_ref().map(|r| r.details.clone())
}

/// 浮窗渲染完成，`content_height` 为内容高度（逻辑像素）。此时定位并显示（不激活）。
pub fn ready<R: Runtime>(window: &WebviewWindow<R>, content_height: f64) {
    let anchor_top = {
        let state = window.state::<DetailsState>();
        let Ok(request) = state.request.lock() else {
            return;
        };
        // 渲染期间已经取消（鼠标移开）时不显示。
        let Some(request) = request.as_ref() else {
            return;
        };
        request.anchor_top
    };
    let Some(main) = window.get_webview_window(MAIN) else {
        return;
    };
    if let Err(e) = place(window, &main, anchor_top, content_height) {
        eprintln!("[details] 显示浮窗失败：{e}");
    }
}

/// 在应用行上按下或滚动时立即隐藏。
pub fn hide<R: Runtime, M: Manager<R>>(manager: &M) {
    let state = manager.state::<DetailsState>();
    state.hide.next();
    state.hovered.store(false, Ordering::SeqCst);
    if let Ok(mut request) = state.request.lock() {
        *request = None;
    }
    if let Some(window) = manager.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// 鼠标离开应用行或浮窗：稍等片刻，鼠标不在浮窗上才隐藏。
/// 隐藏后若主窗口已失去焦点（鼠标在浮窗上期间点了别处），主窗口也按失焦处理。
pub fn hide_soon<R: Runtime>(app: &AppHandle<R>) {
    let generation = app.state::<DetailsState>().hide.next();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(HIDE_DELAY).await;
        let state = app.state::<DetailsState>();
        if !state.hide.is_current(generation) || state.hovered.load(Ordering::SeqCst) {
            return;
        }
        hide(&app);
        crate::window::hide_if_unfocused(&app);
    });
}

/// 浮窗前端报告鼠标移入 / 移出浮窗。
pub fn set_hovered<R: Runtime>(app: &AppHandle<R>, hovered: bool) {
    let state = app.state::<DetailsState>();
    state.hovered.store(hovered, Ordering::SeqCst);
    if hovered {
        state.hide.next();
    } else {
        hide_soon(app);
    }
}

/// 鼠标是否在浮窗上。此时主窗口失焦也不隐藏。
pub fn is_hovered<R: Runtime, M: Manager<R>>(manager: &M) -> bool {
    manager
        .try_state::<DetailsState>()
        .is_some_and(|s| s.hovered.load(Ordering::SeqCst))
}

/// 主窗口隐藏时销毁浮窗，释放它的网页进程内存。
pub fn destroy<R: Runtime, M: Manager<R>>(manager: &M) {
    let state = manager.state::<DetailsState>();
    state.hide.next();
    state.hovered.store(false, Ordering::SeqCst);
    if let Ok(mut request) = state.request.lock() {
        *request = None;
    }
    if let Some(window) = manager.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// 以主窗口的配置为模板创建浮窗（初始隐藏）：同样无边框、Mica 背景、不在任务栏显示。
/// WebView2 启动参数必须与主窗口一致，否则两者无法共用浏览器进程，创建会失败。
fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .expect("tauri.conf.json 中缺少主窗口配置")
        .clone();
    config.label = LABEL.into();
    config.title = "应用详情".into();
    config.width = WIDTH;
    config.height = 160.0;
    config.min_width = None;
    config.max_width = None;
    config.visible = false;
    config.focus = false;
    let window = WebviewWindowBuilder::from_config(app, &config)?
        .additional_browser_args(&crate::window::browser_args_for(app))
        .theme(crate::window::theme_for(app))
        .build()?;
    set_no_activate(&window);
    // 与主窗口相同的右键菜单：复制文字、复制路径等。
    let handle = app.clone();
    let _ = window.with_webview(move |webview| {
        crate::context_menu::install(&handle, &webview.controller(), &webview.environment())
    });
    Ok(())
}

/// 点击或显示时都不成为前台窗口，主窗口保持焦点；也不出现在 Alt+Tab 中。
fn set_no_activate<R: Runtime>(window: &WebviewWindow<R>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = HWND(hwnd.0);
    // SAFETY: hwnd 来自刚创建的窗口，只追加扩展样式位。
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize,
        );
    }
}

/// 按主窗口位置计算浮窗位置，调整尺寸并在不激活的情况下显示。
fn place<R: Runtime>(
    window: &WebviewWindow<R>,
    main: &WebviewWindow<R>,
    anchor_top: f64,
    content_height: f64,
) -> tauri::Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
    };

    let scale = main.scale_factor()?;
    let position = main.outer_position()?;
    let size = main.outer_size()?;
    let main_rect = Rect {
        x: position.x.into(),
        y: position.y.into(),
        width: size.width.into(),
        height: size.height.into(),
    };
    let Some(monitor) = main.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let work_area = Rect {
        x: area.position.x.into(),
        y: area.position.y.into(),
        width: area.size.width.into(),
        height: area.size.height.into(),
    };
    // 尺寸按内容区设置（与主窗口一致）：`SetWindowPos` 设置的是含边框的外框，内容区会略小，出现滚动条。
    window.set_size(tauri::LogicalSize::new(WIDTH, content_height))?;
    let outer = window.outer_size()?;
    let width = outer.width.into();
    let height = outer.height.into();
    let (x, y) = position_beside(
        main_rect,
        work_area,
        width,
        height,
        anchor_top * scale,
        GAP * scale,
    );

    let hwnd = HWND(window.hwnd()?.0);
    // SAFETY: hwnd 来自存活的浮窗。设置位置并显示，不激活窗口。
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x.round() as i32,
            y.round() as i32,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    }
    .map_err(|e| tauri::Error::Anyhow(e.into()))
}

/// 浮窗左上角位置（物理像素）：优先放在主窗口左侧，放不下时放在右侧；
/// 上边缘与应用行对齐，并限制在工作区内。
fn position_beside(
    main: Rect,
    work_area: Rect,
    width: f64,
    height: f64,
    anchor_top: f64,
    gap: f64,
) -> (f64, f64) {
    let left = main.x - gap - width;
    let x = if left >= work_area.x {
        left
    } else {
        (main.x + main.width + gap).min(work_area.x + work_area.width - width)
    };
    let bottom = work_area.y + work_area.height - height;
    let y = (main.y + anchor_top).min(bottom).max(work_area.y);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1032.0,
    };

    fn main_at(x: f64) -> Rect {
        Rect {
            x,
            y: 400.0,
            width: 340.0,
            height: 600.0,
        }
    }

    #[test]
    fn 优先显示在主窗口左侧并与应用行对齐() {
        let (x, y) = position_beside(main_at(1568.0), WORK, 300.0, 200.0, 120.0, 8.0);
        assert_eq!(x, 1568.0 - 8.0 - 300.0);
        assert_eq!(y, 520.0);
    }

    #[test]
    fn 左侧放不下时显示在右侧() {
        let (x, _) = position_beside(main_at(100.0), WORK, 300.0, 200.0, 0.0, 8.0);
        assert_eq!(x, 100.0 + 340.0 + 8.0);
    }

    #[test]
    fn 不超出工作区底部和顶部() {
        let (_, y) = position_beside(main_at(1568.0), WORK, 300.0, 200.0, 580.0, 8.0);
        assert_eq!(y, 1032.0 - 200.0, "应用行靠近底部时浮窗向上移");
        let (_, y) = position_beside(main_at(1568.0), WORK, 300.0, 2000.0, 0.0, 8.0);
        assert_eq!(y, 0.0, "浮窗比工作区还高时贴住顶部");
    }
}
