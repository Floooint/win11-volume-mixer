//! 网页右键菜单（WebView2 原生菜单）：
//! - 去掉对本程序没有意义的“返回”“共享”“另存为”“打印”；
//! - “检查”只在设置中开启了调试工具时显示；
//! - 在最前面加入前端给出的菜单项：复制（应用名、进程名、路径或点中的文字）和动作（置顶、隐藏等）。
//!
//! 前端的 `contextmenu` 监听先于本菜单执行，把菜单项写入 `window.__contextMenuItems`。
//! 这里用 deferral 暂停菜单弹出，通过 `ExecuteScript` 读取后再补齐菜单项。
//! 复制由后端写入剪贴板；动作通过 `window.__contextMenuAction(action)` 交回前端执行。

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU, ICoreWebView2, ICoreWebView2_11,
    ICoreWebView2ContextMenuItem, ICoreWebView2ContextMenuItemCollection,
    ICoreWebView2ContextMenuRequestedEventArgs, ICoreWebView2Controller, ICoreWebView2Environment,
    ICoreWebView2Environment9,
};
use webview2_com::{
    ContextMenuRequestedEventHandler, CustomItemSelectedEventHandler, ExecuteScriptCompletedHandler,
};
use windows_core::{HSTRING, Interface, PWSTR};

use crate::audio::take_pwstr;
use crate::config::Config;

/// 始终移除的菜单项，取值见 WebView2 文档中 `ICoreWebView2ContextMenuItem::Name` 的列表。
const REMOVED: [&str; 4] = ["back", "share", "saveAs", "print"];
/// 只在调试工具开启时保留。
const DEBUG_ONLY: [&str; 1] = ["inspectElement"];

/// 前端给出的一项菜单，见 src/lib/context-menu.ts。
#[derive(Deserialize, Default)]
#[serde(default)]
struct MenuEntry {
    label: String,
    /// 复制到剪贴板的内容。
    value: Option<String>,
    /// 交给前端执行的动作标识。
    action: Option<String>,
    separator: bool,
    /// 子菜单项，如“添加到分组”下的各个分组。
    children: Option<Vec<MenuEntry>>,
}

/// 在 WebView 创建后调用一次。
pub fn install<R: Runtime>(
    app: &AppHandle<R>,
    controller: &ICoreWebView2Controller,
    environment: &ICoreWebView2Environment,
) {
    if let Err(e) = try_install(app.clone(), controller, environment) {
        eprintln!("[context-menu] 无法修改右键菜单：{e}");
    }
}

fn try_install<R: Runtime>(
    app: AppHandle<R>,
    controller: &ICoreWebView2Controller,
    environment: &ICoreWebView2Environment,
) -> windows_core::Result<()> {
    let webview: ICoreWebView2_11 = unsafe { controller.CoreWebView2()? }.cast()?;
    // 较旧的 WebView2 运行时不支持自定义菜单项，此时只做删减。
    let environment: Option<ICoreWebView2Environment9> = environment.cast().ok();
    let handler = ContextMenuRequestedEventHandler::create(Box::new(move |sender, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let items = unsafe { args.MenuItems()? };
        let debug = app.state::<Config>().get().debug_tools;
        remove_items(&items, debug)?;
        if let (Some(sender), Some(environment)) = (sender, &environment) {
            add_frontend_items(sender, args, items, environment.clone())?;
        }
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { webview.add_ContextMenuRequested(&handler, &mut token) }
}

fn remove_items(
    items: &ICoreWebView2ContextMenuItemCollection,
    debug: bool,
) -> windows_core::Result<()> {
    let mut count = 0u32;
    unsafe { items.Count(&mut count)? };
    // 倒序遍历，删除后前面的索引不变。
    for index in (0..count).rev() {
        let item = unsafe { items.GetValueAtIndex(index)? };
        let mut name = PWSTR::null();
        unsafe { item.Name(&mut name)? };
        // SAFETY: Name 返回 CoTaskMemAlloc 分配的字符串，由调用方释放。
        let name = unsafe { take_pwstr(name) };
        if should_remove(&name, debug) {
            unsafe { items.RemoveValueAtIndex(index)? };
        }
    }
    trim_separators(items)
}

fn should_remove(name: &str, debug: bool) -> bool {
    REMOVED.contains(&name) || (!debug && DEBUG_ONLY.contains(&name))
}

/// 删除后可能在开头、结尾或中间留下多余的分隔线。
fn trim_separators(items: &ICoreWebView2ContextMenuItemCollection) -> windows_core::Result<()> {
    let mut count = 0u32;
    unsafe { items.Count(&mut count)? };
    let mut previous_is_separator = true; // 开头的分隔线也删掉
    let mut index = 0;
    while index < count {
        let item = unsafe { items.GetValueAtIndex(index)? };
        let mut kind = Default::default();
        unsafe { item.Kind(&mut kind)? };
        let is_separator = kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR;
        if is_separator && (previous_is_separator || index + 1 == count) {
            unsafe { items.RemoveValueAtIndex(index)? };
            count -= 1;
            continue;
        }
        previous_is_separator = is_separator;
        index += 1;
    }
    Ok(())
}

/// 暂停菜单弹出，读取前端给出的菜单项，插入到菜单最前面后再弹出。
fn add_frontend_items(
    webview: ICoreWebView2,
    args: ICoreWebView2ContextMenuRequestedEventArgs,
    items: ICoreWebView2ContextMenuItemCollection,
    environment: ICoreWebView2Environment9,
) -> windows_core::Result<()> {
    let deferral = unsafe { args.GetDeferral()? };
    let target = webview.clone();
    let completed = ExecuteScriptCompletedHandler::create(Box::new(move |_, json| {
        // 结果是 JSON；脚本出错或没有内容时为 "null" 等，按没有菜单项处理。
        let entries: Vec<MenuEntry> = serde_json::from_str(&json).unwrap_or_default();
        if let Err(e) = insert_entries(&items, &environment, &target, entries) {
            eprintln!("[context-menu] 无法添加菜单项：{e}");
        }
        unsafe { deferral.Complete() }
    }));
    let script = HSTRING::from("window.__contextMenuItems ?? []");
    unsafe { webview.ExecuteScript(&script, &completed) }
}

/// 去掉无效项，以及开头、结尾和连续的分隔线。
fn clean_entries(entries: Vec<MenuEntry>) -> Vec<MenuEntry> {
    let mut result: Vec<MenuEntry> = Vec::new();
    for entry in entries {
        let entry = MenuEntry {
            children: entry.children.map(clean_entries),
            ..entry
        };
        let valid = if entry.separator {
            result.last().is_some_and(|last| !last.separator)
        } else if let Some(children) = &entry.children {
            !children.is_empty()
        } else {
            entry.action.is_some() || entry.value.as_deref().is_some_and(|v| !v.is_empty())
        };
        if valid {
            result.push(entry);
        }
    }
    if result.last().is_some_and(|last| last.separator) {
        result.pop();
    }
    result
}

fn insert_entries(
    items: &ICoreWebView2ContextMenuItemCollection,
    environment: &ICoreWebView2Environment9,
    webview: &ICoreWebView2,
    entries: Vec<MenuEntry>,
) -> windows_core::Result<()> {
    let entries = clean_entries(entries);
    if entries.is_empty() {
        return Ok(());
    }
    let mut count = 0u32;
    unsafe { items.Count(&mut count)? };
    if count > 0 {
        unsafe { items.InsertValueAtIndex(0, &separator(environment)?)? };
    }
    insert_at(items, 0, environment, webview, entries)
}

/// 从 `start` 开始依次插入菜单项，子菜单递归插入。
fn insert_at(
    items: &ICoreWebView2ContextMenuItemCollection,
    start: u32,
    environment: &ICoreWebView2Environment9,
    webview: &ICoreWebView2,
    entries: Vec<MenuEntry>,
) -> windows_core::Result<()> {
    for (index, mut entry) in entries.into_iter().enumerate() {
        let item = if entry.separator {
            separator(environment)?
        } else if let Some(children) = entry.children.take() {
            let submenu = unsafe {
                environment.CreateContextMenuItem(
                    &HSTRING::from(entry.label.as_str()),
                    None,
                    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU,
                )?
            };
            let sub_items = unsafe { submenu.Children()? };
            insert_at(&sub_items, 0, environment, webview, children)?;
            submenu
        } else {
            command(environment, webview, entry)?
        };
        unsafe { items.InsertValueAtIndex(start + index as u32, &item)? };
    }
    Ok(())
}

fn separator(
    environment: &ICoreWebView2Environment9,
) -> windows_core::Result<ICoreWebView2ContextMenuItem> {
    unsafe {
        environment.CreateContextMenuItem(
            &HSTRING::new(),
            None,
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
        )
    }
}

fn command(
    environment: &ICoreWebView2Environment9,
    webview: &ICoreWebView2,
    entry: MenuEntry,
) -> windows_core::Result<ICoreWebView2ContextMenuItem> {
    let item = unsafe {
        environment.CreateContextMenuItem(
            &HSTRING::from(entry.label.as_str()),
            None,
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
        )?
    };
    let webview = webview.clone();
    let selected = CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
        if let Some(value) = &entry.value {
            crate::clipboard::set_text(value);
        }
        if let Some(action) = &entry.action {
            run_action(&webview, action);
        }
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { item.add_CustomItemSelected(&selected, &mut token)? };
    Ok(item)
}

/// 把动作交回前端执行。动作标识经 JSON 编码后拼进脚本，不会被当作代码执行。
fn run_action(webview: &ICoreWebView2, action: &str) {
    let Ok(action) = serde_json::to_string(action) else {
        return;
    };
    let script = HSTRING::from(format!("window.__contextMenuAction?.({action})"));
    let done = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
    if let Err(e) = unsafe { webview.ExecuteScript(&script, &done) } {
        eprintln!("[context-menu] 无法执行菜单动作：{e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 检查只在调试模式下保留() {
        assert!(should_remove("inspectElement", false));
        assert!(!should_remove("inspectElement", true));
    }

    #[test]
    fn 返回共享另存为打印始终移除() {
        for name in ["back", "share", "saveAs", "print"] {
            assert!(should_remove(name, true), "{name}");
        }
        assert!(!should_remove("reload", false));
    }

    #[test]
    fn 前端内容解析失败时视为没有菜单项() {
        let parsed: Vec<MenuEntry> = serde_json::from_str("null").unwrap_or_default();
        assert!(parsed.is_empty());
        let parsed: Vec<MenuEntry> =
            serde_json::from_str(r#"[{"label":"复制应用名","value":"Edge"}]"#).unwrap();
        assert_eq!(parsed[0].label, "复制应用名");
        assert!(!parsed[0].separator);
    }

    #[test]
    fn 清理多余的分隔线和空项() {
        let parsed: Vec<MenuEntry> = serde_json::from_str(
            r#"[{"separator":true},{"label":"复制","value":"a"},{"separator":true},
                {"separator":true},{"label":"空","value":""},{"label":"置顶","action":"pin"},
                {"separator":true}]"#,
        )
        .unwrap();
        let labels: Vec<_> = clean_entries(parsed)
            .into_iter()
            .map(|e| {
                if e.separator {
                    "-".to_string()
                } else {
                    e.label
                }
            })
            .collect();
        assert_eq!(labels, ["复制", "-", "置顶"]);
    }

    #[test]
    fn 空的子菜单被去掉() {
        let parsed: Vec<MenuEntry> = serde_json::from_str(
            r#"[{"label":"添加到分组","children":[{"separator":true}]},
                {"label":"移到分组","children":[{"label":"游戏","action":"group:g1"}]}]"#,
        )
        .unwrap();
        let cleaned = clean_entries(parsed);
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0].label, "移到分组");
        assert_eq!(cleaned[0].children.as_ref().map(Vec::len), Some(1));
    }
}
