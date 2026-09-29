//! 默认输出设备、系统总音量，以及设备变化通知。

use std::sync::mpsc::Sender;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{ERROR_NOT_FOUND, PROPERTYKEY};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole, IAudioSessionManager2,
    IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, eConsole,
    eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, STGM_READ};
use windows_core::{PCWSTR, Result, implement};

use super::types::VolumeState;
use super::{EVENT_CONTEXT, Msg, take_pwstr};

pub struct Device {
    pub id: String,
    pub name: String,
    pub sessions: IAudioSessionManager2,
    endpoint: IAudioEndpointVolume,
    volume_callback: IAudioEndpointVolumeCallback,
}

impl Device {
    /// 打开当前默认输出设备；没有任何输出设备时返回 `None`。
    pub fn open_default(
        enumerator: &IMMDeviceEnumerator,
        tx: &Sender<Msg>,
    ) -> Result<Option<Self>> {
        let device = match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
            Ok(device) => device,
            Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => return Ok(None),
            Err(e) => return Err(e),
        };

        let id = unsafe { take_pwstr(device.GetId()?) };
        let name = friendly_name(&device).unwrap_or_else(|_| "未知设备".into());
        let endpoint: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None)? };
        let sessions: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None)? };

        let volume_callback: IAudioEndpointVolumeCallback =
            VolumeCallback { tx: tx.clone() }.into();
        unsafe { endpoint.RegisterControlChangeNotify(&volume_callback)? };

        Ok(Some(Self {
            id,
            name,
            sessions,
            endpoint,
            volume_callback,
        }))
    }

    pub fn master(&self) -> Result<VolumeState> {
        unsafe {
            Ok(VolumeState {
                volume: self.endpoint.GetMasterVolumeLevelScalar()?,
                muted: self.endpoint.GetMute()?.as_bool(),
            })
        }
    }

    pub fn set_master_volume(&self, volume: f32) -> Result<()> {
        unsafe {
            self.endpoint
                .SetMasterVolumeLevelScalar(volume, &EVENT_CONTEXT)
        }
    }

    pub fn set_master_mute(&self, muted: bool) -> Result<()> {
        unsafe { self.endpoint.SetMute(muted, &EVENT_CONTEXT) }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = unsafe {
            self.endpoint
                .UnregisterControlChangeNotify(&self.volume_callback)
        };
    }
}

fn friendly_name(device: &IMMDevice) -> Result<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ)?;
        let value = store.GetValue(&PKEY_Device_FriendlyName)?;
        Ok(value.to_string())
    }
}

/// 系统总音量变化回调，运行在系统线程上。
#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    tx: Sender<Msg>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        if let Some(data) = unsafe { data.as_ref() } {
            let _ = self.tx.send(Msg::MasterChanged {
                is_self: data.guidEventContext == EVENT_CONTEXT,
            });
        }
        Ok(())
    }
}

/// 设备变化回调。按文档要求，这里不调用任何 Core Audio 接口。
#[implement(IMMNotificationClient)]
pub struct DeviceNotifier {
    pub tx: Sender<Msg>,
}

impl IMMNotificationClient_Impl for DeviceNotifier_Impl {
    fn OnDeviceStateChanged(&self, _device_id: &PCWSTR, _state: DEVICE_STATE) -> Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _device_id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _device_id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _device_id: &PCWSTR,
    ) -> Result<()> {
        // 每个角色都会各通知一次，只关心 eConsole，避免重复重建。
        if flow == eRender && role == eConsole {
            let _ = self.tx.send(Msg::DefaultDeviceChanged);
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _device_id: &PCWSTR, key: &PROPERTYKEY) -> Result<()> {
        // 设备在系统设置中被改名。其他属性变化很频繁，不处理。
        // 不区分是否为当前设备：改名很少发生，按设备变化整体重建即可。
        if *key == PKEY_Device_FriendlyName {
            let _ = self.tx.send(Msg::DefaultDeviceChanged);
        }
        Ok(())
    }
}
