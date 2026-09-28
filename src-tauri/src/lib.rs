mod audio;
mod commands;
mod error;
mod events;
mod tray;
mod window;

use tauri::{Manager, RunEvent};
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::audio::AudioService;

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
        ])
        .events(collect_events![
            events::AudioSnapshotEvent,
            events::MasterChangedEvent,
            events::AppUpsertEvent,
            events::AppRemoveEvent,
        ])
}

/// 生成 `src/bindings.ts`。debug 构建启动时自动执行，也可通过 `cargo test` 触发。
pub fn export_bindings() {
    specta_builder()
        .export(
            specta_typescript::Typescript::default()
                .header("// 此文件由 tauri-specta 自动生成，请勿手动修改。"),
            "../src/bindings.ts",
        )
        .expect("导出 TypeScript 绑定失败");
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
        .manage(window::HiddenAt::default())
        .invoke_handler(builder.invoke_handler())
        .on_window_event(window::handle_event)
        .setup(move |app| {
            builder.mount_events(app);

            let handle = app.handle().clone();
            app.manage(AudioService::start(move |update| {
                events::emit(&handle, update)
            }));
            tray::create(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            // 在进程退出前注销 COM 回调并释放音频资源。
            app.state::<AudioService>().shutdown();
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn 导出前端类型绑定() {
        super::export_bindings();
    }
}
