//! 单个音频会话的跟踪，以及会话相关回调。

use std::sync::mpsc::Sender;

use windows::Win32::Media::Audio::{
    AudioSessionDisconnectReason, AudioSessionState, AudioSessionStateActive, IAudioSessionControl,
    IAudioSessionControl2, IAudioSessionEvents, IAudioSessionEvents_Impl,
    IAudioSessionNotification, IAudioSessionNotification_Impl, ISimpleAudioVolume,
};
use windows_core::{BOOL, GUID, Interface, PCWSTR, Ref, Result, implement};

use super::app_info::{self, AppInfo};
use super::types::VolumeState;
use super::{EVENT_CONTEXT, Msg, take_pwstr};

pub struct TrackedSession {
    pub app: AppInfo,
    pub state: AudioSessionState,
    control: IAudioSessionControl2,
    volume: ISimpleAudioVolume,
    events: IAudioSessionEvents,
}

impl TrackedSession {
    pub fn new(
        control: IAudioSessionControl2,
        session_id: String,
        tx: &Sender<Msg>,
    ) -> Result<Self> {
        let volume: ISimpleAudioVolume = control.cast()?;
        let app = app_info::resolve(&control);

        // 先注册回调再读取状态：反过来的话，两步之间的状态变化（开始播放、过期）会丢失。
        let events: IAudioSessionEvents = SessionEvents {
            session_id,
            tx: tx.clone(),
        }
        .into();
        unsafe { control.RegisterAudioSessionNotification(&events)? };
        let state = match unsafe { control.GetState() } {
            Ok(state) => state,
            Err(e) => {
                let _ = unsafe { control.UnregisterAudioSessionNotification(&events) };
                return Err(e);
            }
        };

        Ok(Self {
            app,
            state,
            control,
            volume,
            events,
        })
    }

    pub fn is_active(&self) -> bool {
        self.state == AudioSessionStateActive
    }

    pub fn volume(&self) -> Result<VolumeState> {
        unsafe {
            Ok(VolumeState {
                volume: self.volume.GetMasterVolume()?,
                muted: self.volume.GetMute()?.as_bool(),
            })
        }
    }

    pub fn set_volume(&self, volume: f32) -> Result<()> {
        unsafe { self.volume.SetMasterVolume(volume, &EVENT_CONTEXT) }
    }

    pub fn set_mute(&self, muted: bool) -> Result<()> {
        unsafe { self.volume.SetMute(muted, &EVENT_CONTEXT) }
    }
}

impl Drop for TrackedSession {
    fn drop(&mut self) {
        let _ = unsafe {
            self.control
                .UnregisterAudioSessionNotification(&self.events)
        };
    }
}

/// 会话实例标识，在会话存续期间唯一。
pub fn instance_id(control: &IAudioSessionControl2) -> Result<String> {
    Ok(unsafe { take_pwstr(control.GetSessionInstanceIdentifier()?) })
}

#[implement(IAudioSessionEvents)]
struct SessionEvents {
    session_id: String,
    tx: Sender<Msg>,
}

impl IAudioSessionEvents_Impl for SessionEvents_Impl {
    fn OnDisplayNameChanged(&self, _name: &PCWSTR, _context: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnIconPathChanged(&self, _path: &PCWSTR, _context: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnSimpleVolumeChanged(
        &self,
        _volume: f32,
        _muted: BOOL,
        context: *const GUID,
    ) -> Result<()> {
        let _ = self.tx.send(Msg::SessionVolumeChanged {
            is_self: unsafe { context.as_ref() } == Some(&EVENT_CONTEXT),
        });
        Ok(())
    }

    fn OnChannelVolumeChanged(
        &self,
        _count: u32,
        _volumes: *const f32,
        _changed: u32,
        _context: *const GUID,
    ) -> Result<()> {
        Ok(())
    }

    fn OnGroupingParamChanged(&self, _param: *const GUID, _context: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnStateChanged(&self, state: AudioSessionState) -> Result<()> {
        let _ = self.tx.send(Msg::SessionStateChanged {
            session_id: self.session_id.clone(),
            state,
        });
        Ok(())
    }

    fn OnSessionDisconnected(&self, _reason: AudioSessionDisconnectReason) -> Result<()> {
        let _ = self.tx.send(Msg::SessionDisconnected {
            session_id: self.session_id.clone(),
        });
        Ok(())
    }
}

/// 新会话通知。回调里拿到的会话对象不跨线程传递，由音频线程重新枚举。
#[implement(IAudioSessionNotification)]
pub struct SessionNotifier {
    pub tx: Sender<Msg>,
}

impl IAudioSessionNotification_Impl for SessionNotifier_Impl {
    fn OnSessionCreated(&self, _session: Ref<IAudioSessionControl>) -> Result<()> {
        let _ = self.tx.send(Msg::SessionCreated);
        Ok(())
    }
}
