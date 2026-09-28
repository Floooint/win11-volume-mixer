//! 音频线程：独占 COM 对象，串行处理请求和回调事件。

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use windows::Win32::Media::Audio::{
    AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionNotification,
    IMMDeviceEnumerator, IMMNotificationClient, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows_core::Interface;

use super::device::{Device, DeviceNotifier};
use super::session::{self, SessionNotifier, TrackedSession};
use super::{Command, Msg, percent};

type Res<T = ()> = Result<T, Box<dyn Error>>;

pub struct AudioHandle {
    tx: Sender<Msg>,
    thread: JoinHandle<()>,
}

impl AudioHandle {
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(Msg::Command(command));
    }

    pub fn shutdown(self) {
        self.send(Command::Shutdown);
        let _ = self.thread.join();
    }
}

pub fn spawn() -> AudioHandle {
    let (tx, rx) = mpsc::channel();
    let thread_tx = tx.clone();
    let thread = thread::Builder::new()
        .name("audio".into())
        .spawn(move || run(thread_tx, rx))
        .expect("无法创建音频线程");
    AudioHandle { tx, thread }
}

fn run(tx: Sender<Msg>, rx: Receiver<Msg>) {
    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        eprintln!("[错误] COM 初始化失败：{e}");
        return;
    }
    // State 必须在 CoUninitialize 之前析构，以便注销所有回调。
    match State::new(tx) {
        Ok(mut state) => state.event_loop(&rx),
        Err(e) => eprintln!("[错误] 音频模块初始化失败：{e}"),
    }
    unsafe { CoUninitialize() };
}

/// 按应用聚合后的视图。
struct AppView {
    app_id: String,
    name: String,
    volume: f32,
    muted: bool,
    active: bool,
    session_count: usize,
}

struct State {
    tx: Sender<Msg>,
    enumerator: IMMDeviceEnumerator,
    device_notifier: IMMNotificationClient,
    device: Option<Device>,
    session_notifier: Option<IAudioSessionNotification>,
    /// 以会话实例标识为键。
    sessions: HashMap<String, TrackedSession>,
    /// 上一次 `list` 输出的应用顺序，命令行按序号选择应用。
    listing: Vec<String>,
}

impl State {
    fn new(tx: Sender<Msg>) -> Res<Self> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device_notifier: IMMNotificationClient = DeviceNotifier { tx: tx.clone() }.into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&device_notifier)? };

        let mut state = Self {
            tx,
            enumerator,
            device_notifier,
            device: None,
            session_notifier: None,
            sessions: HashMap::new(),
            listing: Vec::new(),
        };
        state.open_device()?;
        Ok(state)
    }

    fn event_loop(&mut self, rx: &Receiver<Msg>) {
        while let Ok(msg) = rx.recv() {
            let result = match msg {
                Msg::Command(Command::Shutdown) => break,
                Msg::Command(command) => self.handle_command(command),
                Msg::MasterChanged {
                    volume,
                    muted,
                    is_self,
                } => {
                    print_change(is_self, "系统音量", volume, muted);
                    Ok(())
                }
                Msg::SessionCreated => self.refresh_sessions(true),
                Msg::SessionVolumeChanged {
                    session_id,
                    volume,
                    muted,
                    is_self,
                } => {
                    if let Some(session) = self.sessions.get(&session_id) {
                        print_change(is_self, &session.app.name, volume, muted);
                    }
                    Ok(())
                }
                Msg::SessionStateChanged { session_id, state } => {
                    if state == AudioSessionStateExpired {
                        self.remove_session(&session_id);
                    } else if let Some(session) = self.sessions.get_mut(&session_id) {
                        session.state = state;
                    }
                    Ok(())
                }
                Msg::SessionDisconnected { session_id } => {
                    self.remove_session(&session_id);
                    Ok(())
                }
                Msg::DefaultDeviceChanged => self.reopen_device(),
            };
            if let Err(e) = result {
                eprintln!("[错误] {e}");
            }
        }
    }

    fn handle_command(&mut self, command: Command) -> Res {
        match command {
            Command::List => self.print_list(),
            Command::Refresh => {
                self.refresh_sessions(true)?;
                self.print_list()
            }
            Command::SetMasterVolume(volume) => Ok(self.device()?.set_master_volume(volume)?),
            Command::SetMasterMute(muted) => Ok(self.device()?.set_master_mute(muted)?),
            Command::SetAppVolume(index, volume) => {
                self.for_app_sessions(index, |session| session.set_volume(volume))
            }
            Command::SetAppMute(index, muted) => {
                self.for_app_sessions(index, |session| session.set_mute(muted))
            }
            Command::Shutdown => unreachable!("在 event_loop 中处理"),
        }
    }

    fn device(&self) -> Res<&Device> {
        self.device
            .as_ref()
            .ok_or_else(|| "当前没有输出设备".into())
    }

    fn open_device(&mut self) -> Res {
        self.device = Device::open_default(&self.enumerator, &self.tx)?;
        if let Some(device) = &self.device {
            let notifier: IAudioSessionNotification = SessionNotifier {
                tx: self.tx.clone(),
            }
            .into();
            unsafe { device.sessions.RegisterSessionNotification(&notifier)? };
            self.session_notifier = Some(notifier);
        }
        // 注册新会话通知后必须枚举一次，通知才会开始触发。
        self.refresh_sessions(false)
    }

    fn close_device(&mut self) {
        self.sessions.clear();
        if let (Some(device), Some(notifier)) = (&self.device, &self.session_notifier) {
            let _ = unsafe { device.sessions.UnregisterSessionNotification(notifier) };
        }
        self.session_notifier = None;
        self.device = None;
    }

    fn reopen_device(&mut self) -> Res {
        self.close_device();
        self.open_device()?;
        match &self.device {
            Some(device) => println!("[设备] 默认输出设备已切换为：{}", device.name),
            None => println!("[设备] 当前没有输出设备"),
        }
        self.print_list()
    }

    /// 重新枚举会话：为新会话注册回调，移除已消失的会话。
    fn refresh_sessions(&mut self, announce: bool) -> Res {
        let Some(device) = &self.device else {
            self.sessions.clear();
            return Ok(());
        };
        let list = unsafe { device.sessions.GetSessionEnumerator()? };
        let count = unsafe { list.GetCount()? };

        let mut seen = HashSet::new();
        for i in 0..count {
            let control: IAudioSessionControl2 = unsafe { list.GetSession(i)? }.cast()?;
            if unsafe { control.GetState()? } == AudioSessionStateExpired {
                continue;
            }
            let id = session::instance_id(&control)?;
            seen.insert(id.clone());
            if self.sessions.contains_key(&id) {
                continue;
            }
            match TrackedSession::new(control, id.clone(), &self.tx) {
                Ok(tracked) => {
                    if announce {
                        println!("[新会话] {}（{}）", tracked.app.name, tracked.app.app_id);
                    }
                    self.sessions.insert(id, tracked);
                }
                Err(e) => eprintln!("[错误] 无法跟踪会话：{e}"),
            }
        }

        self.sessions.retain(|id, tracked| {
            let keep = seen.contains(id);
            if !keep && announce {
                println!("[会话结束] {}", tracked.app.name);
            }
            keep
        });
        Ok(())
    }

    fn remove_session(&mut self, session_id: &str) {
        if let Some(tracked) = self.sessions.remove(session_id) {
            println!("[会话结束] {}", tracked.app.name);
        }
    }

    fn apps(&self) -> Vec<AppView> {
        let mut apps: HashMap<&str, AppView> = HashMap::new();
        for tracked in self.sessions.values() {
            let Ok((volume, muted)) = tracked.volume() else {
                continue;
            };
            let view = apps.entry(&tracked.app.app_id).or_insert_with(|| AppView {
                app_id: tracked.app.app_id.clone(),
                name: tracked.app.name.clone(),
                volume: 0.0,
                muted: true,
                active: false,
                session_count: 0,
            });
            // 聚合规则：音量取最大值；所有会话都静音才算静音。
            view.volume = view.volume.max(volume);
            view.muted &= muted;
            view.active |= tracked.is_active();
            view.session_count += 1;
        }

        let mut apps: Vec<_> = apps.into_values().collect();
        apps.sort_by(|a, b| {
            b.active
                .cmp(&a.active)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        apps
    }

    fn print_list(&mut self) -> Res {
        let Some(device) = &self.device else {
            println!("当前没有输出设备");
            self.listing.clear();
            return Ok(());
        };
        let (volume, muted) = device.master()?;
        println!("── 设备：{}", device.name);
        println!("   系统音量 {}{}", percent(volume), mute_mark(muted));

        let apps = self.apps();
        if apps.is_empty() {
            println!("   （没有音频应用）");
        }
        for (i, app) in apps.iter().enumerate() {
            let sessions = if app.session_count > 1 {
                format!("，{} 个会话", app.session_count)
            } else {
                String::new()
            };
            println!(
                "   [{i}] {}{} {}{}  {}{}",
                if app.active { "▶ " } else { "  " },
                app.name,
                percent(app.volume),
                mute_mark(app.muted),
                app.app_id,
                sessions,
            );
        }
        self.listing = apps.into_iter().map(|app| app.app_id).collect();
        Ok(())
    }

    fn for_app_sessions(
        &self,
        index: usize,
        action: impl Fn(&TrackedSession) -> windows_core::Result<()>,
    ) -> Res {
        let app_id = self
            .listing
            .get(index)
            .ok_or("序号无效，请先执行 list 查看应用序号")?;
        let mut matched = false;
        for tracked in self.sessions.values().filter(|s| &s.app.app_id == app_id) {
            action(tracked)?;
            matched = true;
        }
        if !matched {
            return Err("该应用已没有音频会话，请重新执行 list".into());
        }
        Ok(())
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.close_device();
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.device_notifier)
        };
    }
}

fn mute_mark(muted: bool) -> &'static str {
    if muted { "（静音）" } else { "" }
}

fn print_change(is_self: bool, target: &str, volume: f32, muted: bool) {
    // 正式版中自身修改不推送给前端，这里打印出来用于验证防回环。
    let source = if is_self {
        "自身修改，应忽略"
    } else {
        "外部修改"
    };
    println!(
        "[{source}] {target} → {}{}",
        percent(volume),
        mute_mark(muted)
    );
}
