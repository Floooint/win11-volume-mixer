//! 网页右键菜单（WebView2 原生菜单）：
//! - 去掉对本程序没有意义的“返回”“共享”“另存为”“打印”；
//! - “检查”只在设置中开启了调试工具时显示；
//! - 在最前面加入“复制…”项（应用名、进程名、路径或点中的文字），内容由前端在右键时记下。
//!
//! 前端的 `contextmenu` 监听先于本菜单执行，把可复制的内容写入 `window.__contextCopyItems`。
//! 这里用 deferral 暂停菜单弹出，通过 `ExecuteScript` 读取这些内容后再补齐菜单项。

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
    ICoreWebView2, ICoreWebView2_11, ICoreWebView2ContextMenuItemCollection,
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

/// 前端给出的一项可复制内容，如 `{ label: "应用名", value: "Google Chrome" }`。
#[derive(Deserialize)]
struct CopyItem {
    label: String,
    value: String,
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
            add_copy_items(sender, args, items, environment.clone())?;
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

/// 暂停菜单弹出，读取前端记下的可复制内容，插入到菜单最前面后再弹出。
fn add_copy_items(
    webview: ICoreWebView2,
    args: ICoreWebView2ContextMenuRequestedEventArgs,
    items: ICoreWebView2ContextMenuItemCollection,
    environment: ICoreWebView2Environment9,
) -> windows_core::Result<()> {
    let deferral = unsafe { args.GetDeferral()? };
    let completed = ExecuteScriptCompletedHandler::create(Box::new(move |_, json| {
        // 结果是 JSON；脚本出错或没有内容时为 "null" 等，按没有可复制内容处理。
        let copy: Vec<CopyItem> = serde_json::from_str(&json).unwrap_or_default();
        if let Err(e) = insert_copy_items(&items, &environment, copy) {
            eprintln!("[context-menu] 无法添加复制菜单项：{e}");
        }
        unsafe { deferral.Complete() }
    }));
    let script = HSTRING::from("window.__contextCopyItems ?? []");
    unsafe { webview.ExecuteScript(&script, &completed) }
}

fn insert_copy_items(
    items: &ICoreWebView2ContextMenuItemCollection,
    environment: &ICoreWebView2Environment9,
    copy: Vec<CopyItem>,
) -> windows_core::Result<()> {
    let copy: Vec<_> = copy.into_iter().filter(|c| !c.value.is_empty()).collect();
    if copy.is_empty() {
        return Ok(());
    }
    let mut count = 0u32;
    unsafe { items.Count(&mut count)? };
    if count > 0 {
        let separator = unsafe {
            environment.CreateContextMenuItem(
                &HSTRING::new(),
                None,
                COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
            )?
        };
        unsafe { items.InsertValueAtIndex(0, &separator)? };
    }
    for (index, item) in copy.into_iter().enumerate() {
        let entry = unsafe {
            environment.CreateContextMenuItem(
                &HSTRING::from(format!("复制{}", item.label)),
                None,
                COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
            )?
        };
        let value = item.value;
        let selected = CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
            crate::clipboard::set_text(&value);
            Ok(())
        }));
        let mut token = 0i64;
        unsafe { entry.add_CustomItemSelected(&selected, &mut token)? };
        unsafe { items.InsertValueAtIndex(index as u32, &entry)? };
    }
    Ok(())
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
    fn 前端内容解析失败时视为没有可复制内容() {
        let parsed: Vec<CopyItem> = serde_json::from_str("null").unwrap_or_default();
        assert!(parsed.is_empty());
        let parsed: Vec<CopyItem> =
            serde_json::from_str(r#"[{"label":"应用名","value":"Edge"}]"#).unwrap();
        assert_eq!(parsed[0].label, "应用名");
    }
}
