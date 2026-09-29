//! Windows 强调色：读取系统设置并在变化时推送给前端。
//! Windows 11 的控件在浅色主题下使用 AccentDark1，深色主题下使用 AccentLight2，这里取同样的两个值。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use windows::Foundation::TypedEventHandler;
use windows::UI::Color;
use windows::UI::ViewManagement::{UIColorType, UISettings};

/// 浅色、深色主题下的强调色，`#RRGGBB`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "theme://accent")]
pub struct AccentColors {
    pub light: String,
    pub dark: String,
}

/// 持有 `UISettings`，否则变化通知会随对象释放而停止。
#[derive(Default)]
pub struct AccentWatcher(Mutex<Option<UISettings>>);

fn hex(color: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", color.R, color.G, color.B)
}

fn read(settings: &UISettings) -> windows_core::Result<AccentColors> {
    Ok(AccentColors {
        light: hex(settings.GetColorValue(UIColorType::AccentDark1)?),
        dark: hex(settings.GetColorValue(UIColorType::AccentLight2)?),
    })
}

/// 当前系统强调色；读取失败（如精简版系统）时返回 `None`，前端使用内置默认值。
pub fn current() -> Option<AccentColors> {
    UISettings::new()
        .and_then(|settings| read(&settings))
        .inspect_err(|e| eprintln!("[accent] 读取强调色失败：{e}"))
        .ok()
}

/// 监听系统强调色变化并推送 `theme://accent`。通知在系统线程上触发，只做读取和推送。
pub fn watch(app: &AppHandle) {
    let result = (|| -> windows_core::Result<UISettings> {
        let settings = UISettings::new()?;
        let handle = app.clone();
        settings.ColorValuesChanged(&TypedEventHandler::new(
            move |sender: windows_core::Ref<UISettings>, _| {
                if let Some(settings) = sender.as_ref()
                    && let Ok(colors) = read(settings)
                {
                    let _ = colors.emit(&handle);
                }
                Ok(())
            },
        ))?;
        Ok(settings)
    })();
    match result {
        Ok(settings) => {
            if let Ok(mut slot) = app.state::<AccentWatcher>().0.lock() {
                *slot = Some(settings);
            }
        }
        Err(e) => eprintln!("[accent] 无法监听强调色变化：{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 颜色转为十六进制() {
        let color = Color {
            A: 255,
            R: 0,
            G: 0x5F,
            B: 0xB8,
        };
        assert_eq!(hex(color), "#005FB8");
    }

    #[test]
    fn 能读取系统强调色() {
        let colors = current().expect("读取系统强调色失败");
        assert!(colors.light.starts_with('#') && colors.light.len() == 7);
        assert!(colors.dark.starts_with('#') && colors.dark.len() == 7);
    }
}
