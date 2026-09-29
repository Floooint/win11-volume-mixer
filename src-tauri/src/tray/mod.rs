//! 系统托盘：左键显示 / 隐藏窗口，右键菜单打开或退出，中键切换静音，滚轮调节系统音量
//! （可设置为在整个任务栏上响应滚轮）。
//! 图标随系统音量和静音状态变化；样式和颜色可在设置中选择，颜色默认跟随任务栏深浅色。

mod glyph;
mod osd;
mod theme;
mod wheel;

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Position, Size};

use crate::audio::{AudioService, Command, MasterStatus};
use crate::config::Config;
use crate::window::{self, Rect};
use glyph::Icon;

const TRAY_ID: &str = "main";
const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";
/// 设备名很长时截断，Windows 托盘提示最多显示 127 个字符。
const MAX_DEVICE_NAME: usize = 60;

/// 托盘显示的系统音量状态。音频线程写入，主线程据此更新图标。
#[derive(Default)]
pub struct TrayState(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    /// 外层 `None` 表示还没收到过状态，此时保留程序图标。
    status: Option<Option<MasterStatus>>,
    /// 当前图标对应的（图标内容、颜色、尺寸），相同时不重绘。
    icon: Option<(Icon, [u8; 3], u32)>,
    tooltip: String,
}

pub fn create(app: &App) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, MENU_OPEN, "打开", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)?,
        ],
    )?;

    app.manage(TrayState::default());
    let handle = app.handle().clone();
    theme::watch(move || schedule_refresh(&handle));

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("更优雅的音量控制器")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => window::show(app, rect(app)),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } => window::toggle(app, physical_rect(rect)),
                TrayIconEvent::Click {
                    button: MouseButton::Middle,
                    button_state: MouseButtonState::Up,
                    ..
                } => app.state::<AudioService>().post(Command::ToggleMasterMute),
                TrayIconEvent::Enter { rect, .. } | TrayIconEvent::Move { rect, .. } => {
                    wheel::hover(app, physical_rect(rect))
                }
                TrayIconEvent::Leave { .. } => wheel::leave(),
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    wheel::set_taskbar(
        app.handle(),
        app.state::<Config>().read(|s| s.taskbar_wheel),
    );
    Ok(())
}

/// 设置中的“任务栏滚轮”变化后调用。可在任意线程调用，注册 / 注销在主线程上进行。
pub fn set_taskbar_wheel(app: &AppHandle, enabled: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || wheel::set_taskbar(&handle, enabled));
}

/// 托盘图标的位置（物理像素），取不到时为 `None`。
pub fn rect(app: &AppHandle) -> Option<Rect> {
    let tray = app.tray_by_id(TRAY_ID)?;
    tray.rect().ok().flatten().map(physical_rect)
}

/// 记录新的系统音量状态并更新图标。可在任意线程调用：托盘操作转到主线程执行，
/// 不在调用线程上等待主线程（音频线程退出时主线程正等待它，等待会造成死锁）。
pub fn show_status(app: &AppHandle, status: Option<MasterStatus>) {
    if let Some(state) = app.try_state::<TrayState>()
        && let Ok(mut inner) = state.0.lock()
    {
        inner.status = Some(status);
    }
    schedule_refresh(app);
}

/// 设置中的托盘样式或颜色变化后调用。
pub fn schedule_refresh(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || refresh(&handle));
}

fn refresh(app: &AppHandle) {
    let (Some(tray), Some(state)) = (app.tray_by_id(TRAY_ID), app.try_state::<TrayState>()) else {
        return;
    };
    let Ok(mut inner) = state.0.lock() else {
        return;
    };
    let Some(status) = inner.status.clone() else {
        return;
    };

    let (style, color) = app
        .state::<Config>()
        .read(|s| (s.tray_style, s.tray_color.clone()));
    let color =
        color
            .as_deref()
            .and_then(glyph::parse_color)
            .unwrap_or(if theme::taskbar_is_light() {
                [0; 3]
            } else {
                [255; 3]
            });
    let key = (
        Icon::of(style, status.as_ref().map(|s| s.volume)),
        color,
        glyph::icon_size(),
    );
    if inner.icon.as_ref() != Some(&key) {
        let (icon, color, size) = &key;
        // 字体缺失等原因绘制失败时保留原图标。
        if let Some(rgba) = glyph::render(icon, *size, *color) {
            match tray.set_icon(Some(Image::new_owned(rgba, *size, *size))) {
                Ok(()) => inner.icon = Some(key),
                Err(e) => eprintln!("[tray] 更新图标失败：{e}"),
            }
        }
    }

    let tooltip = tooltip(status.as_ref());
    if inner.tooltip != tooltip {
        let _ = tray.set_tooltip(Some(&tooltip));
        inner.tooltip = tooltip;
    }
}

fn tooltip(status: Option<&MasterStatus>) -> String {
    let Some(status) = status else {
        return "未检测到输出设备".into();
    };
    let mut name: String = status.device_name.chars().take(MAX_DEVICE_NAME).collect();
    if name.len() < status.device_name.len() {
        name.push('…');
    }
    if status.volume.muted {
        format!("{name}：已静音")
    } else {
        format!("{name}：{}%", (status.volume.volume * 100.0).round())
    }
}

/// Windows 上托盘事件给出的是物理像素，这里统一转换。
fn physical_rect(rect: tauri::Rect) -> Rect {
    let (x, y) = match rect.position {
        Position::Physical(p) => (p.x.into(), p.y.into()),
        Position::Logical(p) => (p.x, p.y),
    };
    let (width, height) = match rect.size {
        Size::Physical(s) => (s.width.into(), s.height.into()),
        Size::Logical(s) => (s.width, s.height),
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::VolumeState;

    fn status(name: &str, volume: f32, muted: bool) -> MasterStatus {
        MasterStatus {
            device_name: name.into(),
            volume: VolumeState { volume, muted },
        }
    }

    #[test]
    fn 提示文字显示设备名和音量() {
        assert_eq!(
            tooltip(Some(&status("扬声器", 0.364, false))),
            "扬声器：36%"
        );
        assert_eq!(
            tooltip(Some(&status("扬声器", 0.5, true))),
            "扬声器：已静音"
        );
        assert_eq!(tooltip(None), "未检测到输出设备");
    }

    #[test]
    fn 过长的设备名被截断() {
        let long = "长".repeat(100);
        let text = tooltip(Some(&status(&long, 1.0, false)));
        assert_eq!(text, format!("{}…：100%", "长".repeat(MAX_DEVICE_NAME)));
    }
}
