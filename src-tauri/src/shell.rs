//! 在资源管理器中打开文件所在的文件夹并选中该文件（不打开文件本身）。

use std::path::Path;

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};
use windows_core::HSTRING;

use crate::error::{AppError, AppResult, ErrorCode};

/// 打开 `path` 所在的文件夹并选中它。路径必须是存在的绝对路径。
/// 在单独的线程上执行：Shell 接口需要 STA 线程，而且可能等待资源管理器响应。
pub fn reveal_in_folder(path: &str) -> AppResult<()> {
    let path = Path::new(path);
    if !path.is_absolute() || !path.exists() {
        return Err(AppError::new(ErrorCode::ShellFailure, "文件不存在"));
    }
    let path = HSTRING::from(path.as_os_str());
    std::thread::Builder::new()
        .name("reveal".into())
        .spawn(move || {
            // SAFETY: 本线程独占使用 COM，结束前配对调用 CoUninitialize；
            // pidl 由 ILCreateFromPathW 分配，用完后 ILFree 释放。
            unsafe {
                let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
                let pidl = ILCreateFromPathW(&path);
                if pidl.is_null() {
                    eprintln!("[shell] 无法解析路径");
                } else {
                    if let Err(e) = SHOpenFolderAndSelectItems(pidl, None, 0) {
                        eprintln!("[shell] 打开文件夹失败：{e}");
                    }
                    ILFree(Some(pidl));
                }
                if initialized {
                    CoUninitialize();
                }
            }
        })
        .map(|_| ())
        .map_err(|_| AppError::new(ErrorCode::ShellFailure, "无法打开文件夹"))
}
