//! 输出设备列表，以及切换默认输出设备。

use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, DigitalAudioDisplayDevice, ERole, EndpointFormFactor, Handset, Headphones,
    Headset, IMMDevice, IMMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor, eCommunications,
    eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, STGM_READ};
use windows_core::{GUID, HSTRING, PCWSTR, Result};

use super::device::friendly_name;
use super::take_pwstr;
use super::types::{OutputDevice, OutputKind, OutputRole};
use policy_config::IPolicyConfig;

/// 当前可用的输出设备（不含已断开、已禁用的），按名称排序。
pub fn list(enumerator: &IMMDeviceEnumerator) -> Result<Vec<OutputDevice>> {
    let default = default_id(enumerator, eConsole);
    let communications = default_id(enumerator, eCommunications);
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };
    let count = unsafe { collection.GetCount()? };

    let mut outputs = Vec::new();
    for i in 0..count {
        // 单个设备读取失败（通常是刚好被拔出）只跳过它。
        let device = match unsafe { collection.Item(i) } {
            Ok(device) => device,
            Err(e) => {
                eprintln!("[audio] 读取输出设备失败：{e}");
                continue;
            }
        };
        let id = match unsafe { device.GetId() } {
            Ok(id) => unsafe { take_pwstr(id) },
            Err(e) => {
                eprintln!("[audio] 读取输出设备失败：{e}");
                continue;
            }
        };
        outputs.push(OutputDevice {
            is_default: default.as_deref() == Some(id.as_str()),
            is_communications: communications.as_deref() == Some(id.as_str()),
            name: friendly_name(&device).unwrap_or_else(|_| "未知设备".into()),
            kind: kind(&device),
            id,
        });
    }
    outputs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(outputs)
}

/// 把设备设为默认输出设备。切换后系统会发出 `OnDefaultDeviceChanged`，音频线程据此重建。
/// 设置多个角色时只要有一个成功就算成功（界面随之刷新为新状态），失败的只写日志。
pub fn set_default(device_id: &str, role: OutputRole) -> Result<()> {
    let roles: &[ERole] = match role {
        OutputRole::All => &[eConsole, eMultimedia, eCommunications],
        // 与系统“声音”设置中的“设为默认设备”一致，同时设置这两个角色。
        OutputRole::Default => &[eConsole, eMultimedia],
        OutputRole::Communications => &[eCommunications],
    };
    let policy: IPolicyConfig =
        unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)? };
    let id = HSTRING::from(device_id);
    let mut first_error = None;
    let mut any_ok = false;
    for role in roles {
        match unsafe { policy.SetDefaultEndpoint(PCWSTR(id.as_ptr()), role.0) }.ok() {
            Ok(()) => any_ok = true,
            Err(e) => {
                eprintln!("[audio] 设置默认设备失败（角色 {}）：{e}", role.0);
                first_error.get_or_insert(e);
            }
        }
    }
    match first_error {
        Some(e) if !any_ok => Err(e),
        _ => Ok(()),
    }
}

/// 读取失败（如没有默认设备、设备刚好失效）时为 `None`，不影响列出其他设备。
fn default_id(enumerator: &IMMDeviceEnumerator, role: ERole) -> Option<String> {
    let id = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, role) }
        .and_then(|device| unsafe { device.GetId() });
    match id {
        Ok(id) => Some(unsafe { take_pwstr(id) }),
        Err(e) => {
            if e.code() != ERROR_NOT_FOUND.to_hresult() {
                eprintln!("[audio] 读取默认设备失败：{e}");
            }
            None
        }
    }
}

/// 读不到设备类型时按扬声器显示。
fn kind(device: &IMMDevice) -> OutputKind {
    let form_factor = unsafe { device.OpenPropertyStore(STGM_READ) }
        .and_then(|store| unsafe { store.GetValue(&PKEY_AudioEndpoint_FormFactor) })
        .and_then(|value| u32::try_from(&value));
    match form_factor.map(|v| EndpointFormFactor(v as i32)) {
        Ok(f) if f == Headphones || f == Headset || f == Handset => OutputKind::Headphones,
        Ok(f) if f == DigitalAudioDisplayDevice => OutputKind::Display,
        _ => OutputKind::Speakers,
    }
}

/// `CPolicyConfigClient`，见 [`IPolicyConfig`]。
const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

// 方法名与 Windows SDK 中的声明保持一致。
#[allow(non_snake_case)]
mod policy_config {
    use windows_core::{HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, interface};

    /// 未公开的 `IPolicyConfig` 接口：Windows 没有公开设置默认设备的 API，系统自带的“声音”设置
    /// 也通过它切换。Windows 7 起一直未变，EarTrumpet、SoundSwitch 等程序都在使用。
    ///
    /// 只调用 `SetDefaultEndpoint`。它之前的 10 个方法只为占住虚表位置，顺序不能改，
    /// 参数从略（不会调用）。
    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: IUnknown {
        pub fn GetMixFormat(&self) -> HRESULT;
        pub fn GetDeviceFormat(&self) -> HRESULT;
        pub fn ResetDeviceFormat(&self) -> HRESULT;
        pub fn SetDeviceFormat(&self) -> HRESULT;
        pub fn GetProcessingPeriod(&self) -> HRESULT;
        pub fn SetProcessingPeriod(&self) -> HRESULT;
        pub fn GetShareMode(&self) -> HRESULT;
        pub fn SetShareMode(&self) -> HRESULT;
        pub fn GetPropertyValue(&self) -> HRESULT;
        pub fn SetPropertyValue(&self) -> HRESULT;
        pub fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: i32) -> HRESULT;
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Media::Audio::MMDeviceEnumerator;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

    use super::*;

    /// 读取本机的输出设备（不切换），需要手动运行：`cargo test 列出 -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn 列出本机输出设备() {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.unwrap();
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.unwrap();
        // 只确认能创建，不调用 SetDefaultEndpoint。
        let _: IPolicyConfig =
            unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }.unwrap();
        for output in list(&enumerator).unwrap() {
            println!("{output:?}");
        }
    }
}
