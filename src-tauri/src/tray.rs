//! 系统托盘：左键显示 / 隐藏窗口，右键菜单打开或退出。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, Position, Size};

use crate::window::{self, Rect};

const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";

pub fn create(app: &App) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, MENU_OPEN, "打开", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)?,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Win11 声音控制器")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => window::show(app, None),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                window::toggle(tray.app_handle(), physical_rect(rect));
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
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
