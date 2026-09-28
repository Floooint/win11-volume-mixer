//! 音频线程：独占 COM 对象，串行处理请求和回调事件，并把变化合并后推送出去。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;
use windows::Win32::Media::Audio::{
    AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionNotification,
    IMMDeviceEnumerator, IMMNotificationClient, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows_core::Interface;

use super::aggregate::{self, SessionData};
use super::device::{Device, DeviceNotifier};
use super::session::{self, SessionNotifier, TrackedSession};
use super::types::{AudioSnapshot, DeviceInfo, VolumeState};
use super::{Command, Msg, Reply, Update};
use crate::error::{AppError, AppResult, ErrorCode};

/// 变化合并窗口：同一窗口内的多次变化只推送一次。
const COALESCE_WINDOW: Duration = Duration::from_millis(16);

/// 音频服务句柄，由 Tauri 作为全局状态管理。
pub struct AudioService {
    tx: Sender<Msg>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl AudioService {
    /// 启动音频线程。`emit` 在音频线程上调用，用于把变化推送给前端。
    pub fn start(emit: impl Fn(Update) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let thread_tx = tx.clone();
        let thread = thread::Builder::new()
            .name("audio".into())
            .spawn(move || run(thread_tx, rx, Box::new(emit)))
            .expect("无法创建音频线程");
        Self {
            tx,
            thread: Mutex::new(Some(thread)),
        }
    }

    /// 向音频线程发送请求并等待结果。
    pub async fn request<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> AppResult<T> {
        let (reply, result) = oneshot::channel();
        self.tx
            .send(Msg::Command(make(reply)))
            .map_err(|_| thread_down())?;
        result.await.map_err(|_| thread_down())?
    }

    /// 通知音频线程退出并等待其释放 COM 资源。可重复调用。
    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Command(Command::Shutdown));
        let handle = self.thread.lock().ok().and_then(|mut t| t.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

fn thread_down() -> AppError {
    AppError::new(ErrorCode::AudioThreadDown, "音频服务未运行，请重启程序")
}

fn run(tx: Sender<Msg>, rx: Receiver<Msg>, emit: Box<dyn Fn(Update) + Send>) {
    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        eprintln!("[audio] COM 初始化失败：{e}");
        return;
    }
    // State 必须在 CoUninitialize 之前析构，以便注销所有回调。
    match State::new(tx, emit) {
        Ok(mut state) => state.event_loop(&rx),
        Err(e) => eprintln!("[audio] 音频模块初始化失败：{e}"),
    }
    unsafe { CoUninitialize() };
}

/// 待推送的变化。
#[derive(Default)]
struct Pending {
    deadline: Option<Instant>,
    /// 设备切换等情况，需要推送完整快照。
    full: bool,
}

struct State {
    tx: Sender<Msg>,
    emit: Box<dyn Fn(Update) + Send>,
    enumerator: IMMDeviceEnumerator,
    device_notifier: IMMNotificationClient,
    device: Option<Device>,
    session_notifier: Option<IAudioSessionNotification>,
    /// 以会话实例标识为键。
    sessions: HashMap<String, TrackedSession>,
    /// 最近一次推送或返回给前端的状态，用于计算差异。
    last: AudioSnapshot,
    pending: Pending,
}

impl State {
    fn new(tx: Sender<Msg>, emit: Box<dyn Fn(Update) + Send>) -> windows_core::Result<Self> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device_notifier: IMMNotificationClient = DeviceNotifier { tx: tx.clone() }.into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&device_notifier)? };

        let mut state = Self {
            tx,
            emit,
            enumerator,
            device_notifier,
            device: None,
            session_notifier: None,
            sessions: HashMap::new(),
            last: AudioSnapshot {
                device: None,
                apps: Vec::new(),
            },
            pending: Pending::default(),
        };
        state.open_device()?;
        state.last = state.snapshot();
        Ok(state)
    }

    fn event_loop(&mut self, rx: &Receiver<Msg>) {
        loop {
            let first = match self.pending.deadline {
                Some(deadline) => {
                    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(msg) => msg,
                        Err(RecvTimeoutError::Timeout) => {
                            self.flush();
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                None => match rx.recv() {
                    Ok(msg) => msg,
                    Err(_) => return,
                },
            };

            // 一次取出所有积压的消息，合并同一目标的音量请求。
            let batch = coalesce_requests(std::iter::once(first).chain(rx.try_iter()).collect());
            for msg in batch {
                if !self.handle(msg) {
                    return;
                }
            }
            if self
                .pending
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                self.flush();
            }
        }
    }

    /// 返回 `false` 表示应退出事件循环。
    fn handle(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::Command(Command::Shutdown) => return false,
            Msg::Command(command) => self.handle_command(command),
            // 自身修改：前端已持有最新值，不再推送回去，避免滑块回跳。
            Msg::MasterChanged { is_self: true } | Msg::SessionVolumeChanged { is_self: true } => {}
            Msg::MasterChanged { is_self: false }
            | Msg::SessionVolumeChanged { is_self: false } => {
                self.mark_dirty(false);
            }
            Msg::SessionCreated => {
                let result = self.refresh_sessions();
                self.log_err(result);
                self.mark_dirty(false);
            }
            Msg::SessionStateChanged { session_id, state } => {
                if state == AudioSessionStateExpired {
                    self.sessions.remove(&session_id);
                } else if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.state = state;
                }
                self.mark_dirty(false);
            }
            Msg::SessionDisconnected { session_id } => {
                self.sessions.remove(&session_id);
                self.mark_dirty(false);
            }
            Msg::DefaultDeviceChanged => {
                self.close_device();
                let result = self.open_device();
                self.log_err(result);
                self.mark_dirty(true);
            }
        }
        true
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Snapshot(reply) => {
                let snapshot = self.snapshot();
                self.last = snapshot.clone();
                self.pending.full = false;
                let _ = reply.send(Ok(snapshot));
            }
            Command::SetMasterVolume(volume, reply) => {
                let result = self.device().and_then(|d| Ok(d.set_master_volume(volume)?));
                self.reply_after_set(reply, result);
            }
            Command::SetMasterMute(muted, reply) => {
                let result = self.device().and_then(|d| Ok(d.set_master_mute(muted)?));
                self.reply_after_set(reply, result);
            }
            Command::SetAppVolume(app_id, volume, reply) => {
                let result = self.for_app_sessions(&app_id, |s| s.set_volume(volume));
                self.reply_after_set(reply, result);
            }
            Command::SetAppMute(app_id, muted, reply) => {
                let result = self.for_app_sessions(&app_id, |s| s.set_mute(muted));
                self.reply_after_set(reply, result);
            }
            Command::Shutdown => unreachable!("在 handle 中处理"),
        }
    }

    /// 自身修改成功后静默同步基准状态，这样它不会出现在下一次差异中。
    /// 若已有外部变化待推送，则不同步，交给 flush 一并推送。
    fn reply_after_set(&mut self, reply: Reply<()>, result: AppResult<()>) {
        if result.is_ok() && self.pending.deadline.is_none() {
            self.last = self.snapshot();
        }
        let _ = reply.send(result);
    }

    fn mark_dirty(&mut self, full: bool) {
        self.pending.full |= full;
        self.pending
            .deadline
            .get_or_insert_with(|| Instant::now() + COALESCE_WINDOW);
    }

    fn flush(&mut self) {
        let full = std::mem::take(&mut self.pending).full;
        let new = self.snapshot();

        if full || aggregate::device_changed(self.last.device.as_ref(), new.device.as_ref()) {
            (self.emit)(Update::Snapshot(new.clone()));
        } else {
            let diff = aggregate::diff(&self.last, &new);
            if let Some(master) = diff.master {
                (self.emit)(Update::Master(master));
            }
            for app in diff.upserts {
                (self.emit)(Update::AppUpsert(app));
            }
            for app_id in diff.removals {
                (self.emit)(Update::AppRemove(app_id));
            }
        }
        self.last = new;
    }

    fn snapshot(&self) -> AudioSnapshot {
        let Some(device) = &self.device else {
            return AudioSnapshot {
                device: None,
                apps: Vec::new(),
            };
        };
        let master = match device.master() {
            Ok(master) => master,
            Err(e) => {
                eprintln!("[audio] 读取系统音量失败：{e}");
                self.last
                    .device
                    .as_ref()
                    .map(|d| d.master)
                    .unwrap_or(VolumeState {
                        volume: 0.0,
                        muted: false,
                    })
            }
        };
        let apps = aggregate::aggregate(self.sessions.values().filter_map(|session| {
            // 读取失败通常是会话刚好退出，跳过即可。
            let volume = session.volume().ok()?;
            Some(SessionData {
                app: &session.app,
                volume,
                active: session.is_active(),
            })
        }));
        AudioSnapshot {
            device: Some(DeviceInfo {
                id: device.id.clone(),
                name: device.name.clone(),
                master,
            }),
            apps,
        }
    }

    fn device(&self) -> AppResult<&Device> {
        self.device
            .as_ref()
            .ok_or_else(|| AppError::new(ErrorCode::NoDevice, "当前没有输出设备"))
    }

    fn open_device(&mut self) -> windows_core::Result<()> {
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
        self.refresh_sessions()
    }

    fn close_device(&mut self) {
        self.sessions.clear();
        if let (Some(device), Some(notifier)) = (&self.device, &self.session_notifier) {
            let _ = unsafe { device.sessions.UnregisterSessionNotification(notifier) };
        }
        self.session_notifier = None;
        self.device = None;
    }

    /// 重新枚举会话：为新会话注册回调，移除已消失的会话。
    fn refresh_sessions(&mut self) -> windows_core::Result<()> {
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
                    self.sessions.insert(id, tracked);
                }
                Err(e) => eprintln!("[audio] 无法跟踪会话：{e}"),
            }
        }
        self.sessions.retain(|id, _| seen.contains(id));
        Ok(())
    }

    fn for_app_sessions(
        &self,
        app_id: &str,
        action: impl Fn(&TrackedSession) -> windows_core::Result<()>,
    ) -> AppResult<()> {
        let mut matched = false;
        for session in self.sessions.values().filter(|s| s.app.app_id == app_id) {
            action(session)?;
            matched = true;
        }
        if matched {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorCode::AppNotFound,
                "该应用已没有音频会话",
            ))
        }
    }

    fn log_err(&self, result: windows_core::Result<()>) {
        if let Err(e) = result {
            eprintln!("[audio] {e}");
        }
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

#[derive(PartialEq, Eq, Hash)]
enum VolumeTarget {
    Master,
    App(String),
}

/// 同一批消息中，同一目标的音量请求只执行最后一个，其余直接回复成功。
/// 拖动滑块时前端会连续发送请求，这样音频线程不会积压过期的中间值。
fn coalesce_requests(batch: Vec<Msg>) -> Vec<Msg> {
    let target = |msg: &Msg| match msg {
        Msg::Command(Command::SetMasterVolume(..)) => Some(VolumeTarget::Master),
        Msg::Command(Command::SetAppVolume(app_id, ..)) => Some(VolumeTarget::App(app_id.clone())),
        _ => None,
    };

    let mut last_index = HashMap::new();
    for (i, msg) in batch.iter().enumerate() {
        if let Some(target) = target(msg) {
            last_index.insert(target, i);
        }
    }

    batch
        .into_iter()
        .enumerate()
        .filter_map(|(i, msg)| match target(&msg) {
            Some(t) if last_index[&t] != i => {
                if let Msg::Command(
                    Command::SetMasterVolume(_, reply) | Command::SetAppVolume(_, _, reply),
                ) = msg
                {
                    let _ = reply.send(Ok(()));
                }
                None
            }
            _ => Some(msg),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_master(volume: f32) -> (Msg, oneshot::Receiver<AppResult<()>>) {
        let (tx, rx) = oneshot::channel();
        (Msg::Command(Command::SetMasterVolume(volume, tx)), rx)
    }

    fn set_app(app_id: &str, volume: f32) -> (Msg, oneshot::Receiver<AppResult<()>>) {
        let (tx, rx) = oneshot::channel();
        (
            Msg::Command(Command::SetAppVolume(app_id.into(), volume, tx)),
            rx,
        )
    }

    fn volume_of(msg: &Msg) -> f32 {
        match msg {
            Msg::Command(Command::SetMasterVolume(v, _) | Command::SetAppVolume(_, v, _)) => *v,
            _ => panic!("不是音量请求"),
        }
    }

    #[test]
    fn 同一目标只保留最后一个音量请求() {
        let (m1, mut r1) = set_master(0.1);
        let (a1, mut r2) = set_app("edge", 0.2);
        let (m2, _r3) = set_master(0.3);
        let (b1, _r4) = set_app("music", 0.4);
        let (a2, _r5) = set_app("edge", 0.5);

        let kept = coalesce_requests(vec![m1, a1, m2, b1, a2, Msg::SessionCreated]);
        let volumes: Vec<_> = kept.iter().take(3).map(volume_of).collect();
        assert_eq!(volumes, [0.3, 0.4, 0.5]);
        assert!(matches!(kept[3], Msg::SessionCreated));

        // 被丢弃的请求也要收到回复，否则前端的 Promise 永远不会结束。
        assert!(matches!(r1.try_recv(), Ok(Ok(()))));
        assert!(matches!(r2.try_recv(), Ok(Ok(()))));
    }
}
