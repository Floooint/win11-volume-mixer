mod accent;
mod animation;
mod audio;
mod autostart;
mod clipboard;
mod commands;
mod config;
mod context_menu;
mod error;
mod events;
mod feedback;
mod icon;
mod tray;
mod window;

use tauri::{Manager, RunEvent};
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::audio::{AudioService, Update};

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        // 默认会把 f32 导出为 `number | null`（JSON 中 NaN 会变成 null）。
        // 音量在命令入口已限制在 0–1，Windows 返回的值也总是有限数，因此导出为 `number`。
        .semantic_types(
            specta_typescript::semantic::Configuration::empty().enable_lossless_floats(),
        )
        .commands(collect_commands![
            commands::get_snapshot,
            commands::set_master_volume,
            commands::set_master_mute,
            commands::set_app_volume,
            commands::set_app_mute,
            commands::set_group_volume,
            commands::set_group_mute,
            commands::window_ready,
            commands::fit_window_height,
            commands::play_volume_feedback,
            commands::get_refresh_rate,
            commands::get_settings,
            commands::get_default_settings,
            commands::set_settings,
            commands::preview_window_width,
            commands::get_autostart,
            commands::get_pin_mode,
            commands::set_pin_mode,
            commands::get_accent_colors,
            commands::set_autostart,
        ])
        .events(collect_events![
            events::AudioSnapshotEvent,
            events::MasterChangedEvent,
            events::AppUpsertEvent,
            events::AppRemoveEvent,
            accent::AccentColors,
        ])
}

/// 生成 `src/bindings.ts`。debug 构建启动时自动执行，也可通过 `cargo test` 触发。
pub fn export_bindings() {
    const PATH: &str = "../src/bindings.ts";
    specta_builder()
        .export(
            specta_typescript::Typescript::default()
                .header("// 此文件由 tauri-specta 自动生成，请勿手动修改。"),
            PATH,
        )
        .expect("导出 TypeScript 绑定失败");
    fix_optional_array_transforms(PATH);
}

/// 开启无损浮点后，tauri-specta 会为含数字的嵌套数组生成 `x.groups.map(...)` 这样的转换代码，
/// 但设置结构体带 `#[serde(default)]`，这些字段在前端是可选的，直接调用 `.map` 无法通过类型检查。
/// 这些转换只是原样复制（`i=>i`），改为可选链调用即可，运行结果不变。
fn fix_optional_array_transforms(path: &str) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let fixed = text.replace("settings.groups.map(", "settings.groups?.map(");
    if fixed != text {
        let _ = std::fs::write(path, fixed);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    export_bindings();

    let app = tauri::Builder::default()
        // 必须第一个注册：重复启动时唤起已有窗口，新进程随即退出。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            window::show(app, None);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(window::WindowState::default())
        .manage(accent::AccentWatcher::default())
        .manage(feedback::Feedback::default())
        .manage(icon::IconService::start())
        .register_asynchronous_uri_scheme_protocol(icon::SCHEME, |ctx, request, responder| {
            ctx.app_handle()
                .state::<icon::IconService>()
                .respond(request.uri().path(), |response| responder.respond(response));
        })
        .invoke_handler(builder.invoke_handler())
        .on_window_event(window::handle_event)
        .setup(move |app| {
            builder.mount_events(app);

            // 设置必须在创建窗口之前加载：窗口隐藏策略由它决定。
            app.manage(config::Config::load(app.handle()));
            autostart::enable_on_first_run(app.handle());

            let handle = app.handle().clone();
            app.manage(AudioService::start(move |update| match update {
                Update::MasterStatus(status) => tray::show_status(&handle, status),
                update => events::emit(&handle, update),
            }));
            accent::watch(app.handle());
            tray::create(app)?;
            window::init(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");

    app.run(|app, event| match event {
        // 销毁策略下关闭最后一个窗口会触发退出请求（code 为 None），程序应继续在托盘运行。
        // 托盘菜单“退出”调用 `app.exit(0)`，code 为 Some，正常退出。
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => {
            // 在进程退出前注销 COM 回调并释放音频资源。
            app.state::<AudioService>().shutdown();
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn 导出前端类型绑定() {
        super::export_bindings();
    }
}
