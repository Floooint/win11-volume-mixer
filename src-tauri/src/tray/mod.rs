//! 系统托盘：左键显示 / 隐藏窗口，右键菜单打开、静音、切换输出设备或退出，中键切换静音，
//! 滚轮调节系统音量（可设置为在整个任务栏上响应滚轮）。
//! 图标随系统音量和静音状态变化；样式和颜色可在设置中选择，颜色默认跟随任务栏深浅色。

mod glyph;
mod osd;
mod theme;
mod wheel;

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Position, Size};

use crate::audio::{AudioService, Command, MasterStatus, OutputDevice, OutputRole};
use crate::config::Config;
use crate::window::{self, Rect};
use glyph::Icon;

const TRAY_ID: &str = "main";
const MENU_OPEN: &str = "open";
const MENU_MUTE: &str = "mute";
const MENU_QUIT: &str = "quit";
/// 输出设备子菜单中各设备的菜单项标识为此前缀加设备标识。
const MENU_OUTPUT_PREFIX: &str = "output:";
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
    /// 右键菜单中的“静音”，勾选状态随系统静音同步。
    mute_item: Option<CheckMenuItem<tauri::Wry>>,
    /// “静音”当前的（可用、勾选）状态，相同时不更新。
    mute_state: Option<(bool, bool)>,
    /// 音频线程推送的输出设备列表。
    outputs: Vec<OutputDevice>,
    /// 右键菜单中的“输出设备”子菜单。
    outputs_menu: Option<Submenu<tauri::Wry>>,
    /// 子菜单当前的内容（名称、是否勾选），相同时不重建。
    outputs_state: Option<Vec<(String, String, bool)>>,
}

pub fn create(app: &App) -> tauri::Result<()> {
    // 勾选状态在 `refresh` 中按系统静音同步；没有输出设备时不可用。
    let mute = CheckMenuItem::with_id(app, MENU_MUTE, "静音", false, false, None::<&str>)?;
    // 设备在 `refresh` 中按音频线程推送的列表填入；没有可切换的设备时不可用。
    let outputs = Submenu::new(app, "输出设备", false)?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, MENU_OPEN, "打开", true, None::<&str>)?,
            &mute,
            &outputs,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)?,
        ],
    )?;
    if let Ok(mut inner) = app.state::<TrayState>().0.lock() {
        inner.mute_item = Some(mute);
        inner.outputs_menu = Some(outputs);
    }

    let handle = app.handle().clone();
    theme::watch(move || schedule_refresh(&handle));

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("更优雅的音量控制器")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => window::show(app, rect(app)),
            MENU_MUTE => {
                app.state::<AudioService>().post(Command::ToggleMasterMute);
                // 点击时菜单项会自己切换勾选；清掉记录并刷新，按实际静音状态重新同步（切换失败时也不会错）。
                if let Ok(mut inner) = app.state::<TrayState>().0.lock() {
                    inner.mute_state = None;
                }
                schedule_refresh(app);
            }
            MENU_QUIT => app.exit(0),
            id => {
                if let Some(device_id) = id.strip_prefix(MENU_OUTPUT_PREFIX) {
                    let device_id = device_id.to_string();
                    app.state::<AudioService>()
                        .post(|reply| Command::SetDefaultOutput(device_id, OutputRole::All, reply));
                    // 菜单项会自己切换勾选；按实际状态重新同步（设备切换后列表推送过来还会再同步一次）。
                    if let Ok(mut inner) = app.state::<TrayState>().0.lock() {
                        inner.outputs_state = None;
                    }
                    schedule_refresh(app);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Down,
                    ..
                } => window::tray_pressed(app),
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
    // 托盘创建前音频线程可能已报告过音量状态（已记在 `TrayState` 中），创建后按它刷新一次图标。
    schedule_refresh(app.handle());
    wheel::set_taskbar(
        app.handle(),
        app.state::<Config>().read(|s| s.taskbar_wheel),
    );
    wheel::set_prewarm(
        app.handle(),
        app.state::<Config>().read(|s| s.prewarm_on_hover),
    );
    Ok(())
}

/// 设置中的“任务栏滚轮”变化后调用。可在任意线程调用，注册 / 注销在主线程上进行。
pub fn set_taskbar_wheel(app: &AppHandle, enabled: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || wheel::set_taskbar(&handle, enabled));
}

/// 设置中的“悬停任务栏时预加载窗口”变化后调用。可在任意线程调用。
pub fn set_prewarm(app: &AppHandle, enabled: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || wheel::set_prewarm(&handle, enabled));
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

/// 记录新的输出设备列表并更新托盘菜单。可在任意线程调用（同 [`show_status`]）。
pub fn show_outputs(app: &AppHandle, outputs: Vec<OutputDevice>) {
    if let Some(state) = app.try_state::<TrayState>()
        && let Ok(mut inner) = state.0.lock()
    {
        inner.outputs = outputs;
    }
    schedule_refresh(app);
}

/// 设置中的托盘样式、颜色或设备的隐藏、重命名变化后调用。
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

    let (style, color, hidden, aliases) = app.state::<Config>().read(|s| {
        (
            s.tray_style,
            s.tray_color.clone(),
            s.hidden_outputs.clone(),
            s.output_aliases.clone(),
        )
    });
    let display_name = |id: &str, name: &str| {
        aliases
            .iter()
            .find(|a| a.id == id)
            .map_or_else(|| name.to_string(), |a| a.alias.clone())
    };
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

    let tooltip = tooltip(
        status
            .as_ref()
            .map(|s| MasterStatus {
                device_name: display_name(&s.device_id, &s.device_name),
                ..s.clone()
            })
            .as_ref(),
    );
    if inner.tooltip != tooltip {
        let _ = tray.set_tooltip(Some(&tooltip));
        inner.tooltip = tooltip;
    }

    let mute_state = (
        status.is_some(),
        status.as_ref().is_some_and(|s| s.volume.muted),
    );
    if inner.mute_state != Some(mute_state)
        && let Some(item) = &inner.mute_item
    {
        let _ = item.set_enabled(mute_state.0);
        let _ = item.set_checked(mute_state.1);
        inner.mute_state = Some(mute_state);
    }

    // 与主界面一致：不显示设置中隐藏的设备（正在播放或通话的设备除外）。
    let outputs_state: Vec<_> = inner
        .outputs
        .iter()
        .filter(|o| o.is_default || o.is_communications || !hidden.iter().any(|h| h.id == o.id))
        .map(|o| (o.id.clone(), display_name(&o.id, &o.name), o.is_default))
        .collect();
    if inner.outputs_state.as_ref() != Some(&outputs_state)
        && let Some(menu) = &inner.outputs_menu
    {
        if let Err(e) = rebuild_outputs_menu(app, menu, &outputs_state) {
            eprintln!("[tray] 更新输出设备菜单失败：{e}");
        }
        inner.outputs_state = Some(outputs_state);
    }
}

/// 按（设备标识、显示名称、是否当前设备）重建“输出设备”子菜单。只有一个设备时不可用。
fn rebuild_outputs_menu(
    app: &AppHandle,
    menu: &Submenu<tauri::Wry>,
    outputs: &[(String, String, bool)],
) -> tauri::Result<()> {
    for item in menu.items()? {
        menu.remove(&item)?;
    }
    for (id, name, current) in outputs {
        // Win32 菜单把 `&` 当作快捷键标记，设备名中的 `&` 要写成 `&&` 才能显示出来。
        let item = CheckMenuItem::with_id(
            app,
            format!("{MENU_OUTPUT_PREFIX}{id}"),
            name.replace('&', "&&"),
            true,
            *current,
            None::<&str>,
        )?;
        menu.append(&item)?;
    }
    menu.set_enabled(outputs.len() > 1)
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
            device_id: "dev".into(),
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
