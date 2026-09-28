//! 从会话推断所属应用：AppId、显示名称与图标来源。规则见 docs/architecture.md“应用标识与聚合”。

use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, S_OK};
use windows::Win32::Media::Audio::IAudioSessionControl2;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::Storage::Packaging::Appx::{GetApplicationUserModelId, GetPackageFamilyName};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_core::{HSTRING, PCWSTR, PWSTR};

use super::take_pwstr;

#[derive(Debug, Clone)]
pub struct AppInfo {
    pub app_id: String,
    pub name: String,
    /// Shell 解析名（exe 路径或 `shell:AppsFolder\<AUMID>`），由 `icon` 模块提取图标。
    pub icon: Option<String>,
}

pub fn resolve(control: &IAudioSessionControl2) -> AppInfo {
    if unsafe { control.IsSystemSoundsSession() } == S_OK {
        return AppInfo {
            app_id: "system".into(),
            name: "系统声音".into(),
            icon: system_sounds_icon(),
        };
    }

    let pid = unsafe { control.GetProcessId() }.unwrap_or(0);
    let display_name = unsafe { control.GetDisplayName() }
        .map(|p| unsafe { take_pwstr(p) })
        .ok()
        // “@%SystemRoot%\...” 形式的资源引用暂不解析
        .filter(|name| !name.is_empty() && !name.starts_with('@'));

    from_process(pid, display_name)
}

fn from_process(pid: u32, display_name: Option<String>) -> AppInfo {
    let unknown = || AppInfo {
        app_id: format!("pid:{pid}"),
        name: display_name
            .clone()
            .unwrap_or_else(|| format!("未知应用（PID {pid}）")),
        icon: None,
    };

    // 管理员权限进程等情况下会失败，按 PID 回退。
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return unknown();
    };
    let path = image_path(process);
    let family_name = package_family_name(process);
    let aumid = app_user_model_id(process);
    let _ = unsafe { CloseHandle(process) };

    let Some(path) = path else {
        return unknown();
    };
    let name = file_description(&path)
        .or_else(|| display_name.clone())
        .unwrap_or_else(|| file_stem(&path));
    let app_id = family_name.unwrap_or_else(|| path.to_lowercase());
    // 打包应用的 exe 往往没有图标或只是通用图标，改用开始菜单中的应用磁贴图标。
    let icon = Some(aumid.map_or(path, |id| format!(r"shell:AppsFolder\{id}")));

    AppInfo { app_id, name, icon }
}

fn image_path(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// 仅打包（UWP / MSIX）应用有包家族名。
fn package_family_name(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    let result = unsafe { GetPackageFamilyName(process, &mut len, Some(PWSTR(buf.as_mut_ptr()))) };
    if result != ERROR_SUCCESS || len == 0 {
        return None;
    }
    // len 包含结尾的 0
    Some(String::from_utf16_lossy(&buf[..len as usize - 1]))
}

/// 打包应用的 AppUserModelID，例如 `Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic`。
fn app_user_model_id(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    let result =
        unsafe { GetApplicationUserModelId(process, &mut len, Some(PWSTR(buf.as_mut_ptr()))) };
    if result != ERROR_SUCCESS || len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize - 1]))
}

/// 系统声音没有进程路径，借用音量合成器（SndVol.exe）的扬声器图标。
fn system_sounds_icon() -> Option<String> {
    let root = std::env::var("SystemRoot").ok()?;
    Some(format!(r"{root}\System32\SndVol.exe"))
}

/// 读取 exe 版本信息中的 FileDescription，例如“Google Chrome”。
fn file_description(path: &str) -> Option<String> {
    let path = HSTRING::from(path);
    let size = unsafe { GetFileVersionInfoSizeW(&path, None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(&path, None, size, data.as_mut_ptr() as *mut c_void) }.ok()?;

    let block = data.as_ptr() as *const c_void;
    let translation = query_value(block, "\\VarFileInfo\\Translation")
        .filter(|(_, len)| *len >= 4)
        .map(|(ptr, _)| unsafe { *(ptr as *const [u16; 2]) });

    // 优先使用文件声明的语言，其次回退到英语（美国）/ Unicode。
    let mut candidates = Vec::new();
    if let Some([lang, codepage]) = translation {
        candidates.push(format!("{lang:04x}{codepage:04x}"));
    }
    candidates.push("040904b0".into());

    candidates.iter().find_map(|code| {
        let (ptr, len) = query_value(block, &format!("\\StringFileInfo\\{code}\\FileDescription"))?;
        let chars = unsafe { std::slice::from_raw_parts(ptr as *const u16, len as usize) };
        let text = String::from_utf16_lossy(chars);
        let text = text.trim_end_matches('\0').trim();
        (!text.is_empty()).then(|| text.to_string())
    })
}

fn query_value(block: *const c_void, sub_block: &str) -> Option<(*mut c_void, u32)> {
    let sub_block = HSTRING::from(sub_block);
    let mut ptr = std::ptr::null_mut();
    let mut len = 0u32;
    let found = unsafe { VerQueryValueW(block, PCWSTR(sub_block.as_ptr()), &mut ptr, &mut len) };
    (found.as_bool() && !ptr.is_null()).then_some((ptr, len))
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}
