//! 会话按应用聚合，以及两次快照之间的差异计算。不依赖 Windows API，便于单元测试。

use std::collections::HashMap;

use super::app_info::AppInfo;
use super::types::{AppAudio, AudioSnapshot, DeviceInfo, VolumeState};

/// 聚合前的单个会话数据。
pub struct SessionData<'a> {
    pub app: &'a AppInfo,
    pub volume: VolumeState,
    pub active: bool,
}

/// 聚合规则：音量取各会话最大值；所有会话都静音才算静音；任一会话活跃即活跃。
pub fn aggregate<'a>(sessions: impl IntoIterator<Item = SessionData<'a>>) -> Vec<AppAudio> {
    let mut apps: HashMap<&str, AppAudio> = HashMap::new();
    for session in sessions {
        let app = apps.entry(&session.app.app_id).or_insert_with(|| AppAudio {
            app_id: session.app.app_id.clone(),
            name: session.app.name.clone(),
            icon: session.app.icon.clone(),
            process_name: session.app.process_name.clone(),
            exe_path: session.app.exe_path.clone(),
            volume: VolumeState {
                volume: 0.0,
                muted: true,
            },
            active: false,
            session_count: 0,
        });
        app.volume.volume = app.volume.volume.max(session.volume.volume);
        app.volume.muted &= session.volume.muted;
        app.active |= session.active;
        app.session_count += 1;
    }

    let mut apps: Vec<_> = apps.into_values().collect();
    apps.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.app_id.cmp(&b.app_id))
    });
    apps
}

/// 新旧快照之间需要推送给前端的变化。
#[derive(Debug, Default, PartialEq)]
pub struct Diff {
    pub master: Option<VolumeState>,
    pub upserts: Vec<AppAudio>,
    pub removals: Vec<String>,
}

#[cfg(test)]
impl Diff {
    pub fn is_empty(&self) -> bool {
        self.master.is_none() && self.upserts.is_empty() && self.removals.is_empty()
    }
}

/// 设备变化不走差异推送，调用方应改为推送完整快照。
pub fn device_changed(old: Option<&DeviceInfo>, new: Option<&DeviceInfo>) -> bool {
    old.map(|d| &d.id) != new.map(|d| &d.id)
}

pub fn diff(old: &AudioSnapshot, new: &AudioSnapshot) -> Diff {
    let master = match (&old.device, &new.device) {
        (Some(old), Some(new)) if old.master != new.master => Some(new.master),
        _ => None,
    };

    let old_apps: HashMap<&str, &AppAudio> = old
        .apps
        .iter()
        .map(|app| (app.app_id.as_str(), app))
        .collect();
    let upserts = new
        .apps
        .iter()
        .filter(|app| old_apps.get(app.app_id.as_str()) != Some(app))
        .cloned()
        .collect();

    let new_ids: std::collections::HashSet<&str> =
        new.apps.iter().map(|app| app.app_id.as_str()).collect();
    let removals = old
        .apps
        .iter()
        .filter(|app| !new_ids.contains(app.app_id.as_str()))
        .map(|app| app.app_id.clone())
        .collect();

    Diff {
        master,
        upserts,
        removals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(app_id: &str, name: &str) -> AppInfo {
        AppInfo {
            app_id: app_id.into(),
            name: name.into(),
            icon: None,
            process_name: None,
            exe_path: None,
        }
    }

    fn vol(volume: f32, muted: bool) -> VolumeState {
        VolumeState { volume, muted }
    }

    fn session(app: &AppInfo, volume: f32, muted: bool, active: bool) -> SessionData<'_> {
        SessionData {
            app,
            volume: vol(volume, muted),
            active,
        }
    }

    #[test]
    fn 同一应用的多个会话合并为一项() {
        let edge = info("edge.exe", "Edge");
        let apps = aggregate([
            session(&edge, 0.3, false, false),
            session(&edge, 0.8, true, true),
        ]);
        assert_eq!(apps.len(), 1);
        let app = &apps[0];
        assert_eq!(app.volume.volume, 0.8, "音量取最大值");
        assert!(!app.volume.muted, "只有部分会话静音时不算静音");
        assert!(app.active, "任一会话活跃即活跃");
        assert_eq!(app.session_count, 2);
    }

    #[test]
    fn 所有会话都静音才算静音() {
        let edge = info("edge.exe", "Edge");
        let apps = aggregate([
            session(&edge, 0.3, true, false),
            session(&edge, 0.5, true, false),
        ]);
        assert!(apps[0].volume.muted);
    }

    #[test]
    fn 活跃应用在前再按名称排序() {
        let a = info("a", "alpha");
        let b = info("b", "Beta");
        let c = info("c", "charlie");
        let apps = aggregate([
            session(&c, 1.0, false, false),
            session(&b, 1.0, false, true),
            session(&a, 1.0, false, false),
        ]);
        let names: Vec<_> = apps.iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, ["Beta", "alpha", "charlie"]);
    }

    fn snapshot(master: VolumeState, apps: Vec<AppAudio>) -> AudioSnapshot {
        AudioSnapshot {
            device: Some(DeviceInfo {
                id: "dev".into(),
                name: "扬声器".into(),
                master,
            }),
            apps,
        }
    }

    #[test]
    fn 无变化时差异为空() {
        let edge = info("edge", "Edge");
        let apps = aggregate([session(&edge, 0.5, false, true)]);
        let old = snapshot(vol(0.3, false), apps.clone());
        let new = snapshot(vol(0.3, false), apps);
        assert!(diff(&old, &new).is_empty());
    }

    #[test]
    fn 差异包含总音量变化新增修改和移除() {
        let edge = info("edge", "Edge");
        let music = info("music", "Music");
        let steam = info("steam", "Steam");
        let old = snapshot(
            vol(0.3, false),
            aggregate([
                session(&edge, 0.5, false, true),
                session(&steam, 1.0, false, false),
            ]),
        );
        let new = snapshot(
            vol(0.4, false),
            aggregate([
                session(&edge, 0.6, false, true),
                session(&music, 1.0, false, true),
            ]),
        );

        let d = diff(&old, &new);
        assert_eq!(d.master, Some(vol(0.4, false)));
        let mut upserted: Vec<_> = d.upserts.iter().map(|app| app.app_id.as_str()).collect();
        upserted.sort();
        assert_eq!(upserted, ["edge", "music"]);
        assert_eq!(d.removals, ["steam"]);
    }

    #[test]
    fn 设备编号不同才算设备变化() {
        let a = snapshot(vol(0.3, false), vec![]);
        let mut b = a.clone();
        b.device.as_mut().unwrap().master = vol(0.9, true);
        assert!(!device_changed(a.device.as_ref(), b.device.as_ref()));

        b.device.as_mut().unwrap().id = "other".into();
        assert!(device_changed(a.device.as_ref(), b.device.as_ref()));
        assert!(device_changed(a.device.as_ref(), None));
    }
}
