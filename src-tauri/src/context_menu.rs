//! 网页右键菜单：去掉对本程序没有意义的“另存为”和“打印”，其余（如“检查”）保留用于调试。

use webview2_com::ContextMenuRequestedEventHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2_11, ICoreWebView2ContextMenuItemCollection, ICoreWebView2Controller,
};
use windows_core::{Interface, PWSTR};

use crate::audio::take_pwstr;

/// 要移除的菜单项，取值见 WebView2 文档中 `ICoreWebView2ContextMenuItem::Name` 的列表。
const REMOVED: [&str; 2] = ["saveAs", "print"];

/// 在 WebView 创建后调用一次。
pub fn install(controller: &ICoreWebView2Controller) {
    if let Err(e) = try_install(controller) {
        eprintln!("[context-menu] 无法修改右键菜单：{e}");
    }
}

fn try_install(controller: &ICoreWebView2Controller) -> windows_core::Result<()> {
    let webview: ICoreWebView2_11 = unsafe { controller.CoreWebView2()? }.cast()?;
    let handler = ContextMenuRequestedEventHandler::create(Box::new(|_, args| {
        if let Some(args) = args {
            remove_items(&unsafe { args.MenuItems()? })?;
        }
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { webview.add_ContextMenuRequested(&handler, &mut token) }
}

fn remove_items(items: &ICoreWebView2ContextMenuItemCollection) -> windows_core::Result<()> {
    let mut count = 0u32;
    unsafe { items.Count(&mut count)? };
    // 倒序遍历，删除后前面的索引不变。
    for index in (0..count).rev() {
        let item = unsafe { items.GetValueAtIndex(index)? };
        let mut name = PWSTR::null();
        unsafe { item.Name(&mut name)? };
        // SAFETY: Name 返回 CoTaskMemAlloc 分配的字符串，由调用方释放。
        let name = unsafe { take_pwstr(name) };
        if REMOVED.contains(&name.as_str()) {
            unsafe { items.RemoveValueAtIndex(index)? };
        }
    }
    Ok(())
}
