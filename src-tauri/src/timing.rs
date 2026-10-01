//! 新建窗口打开过程的分段计时，用于分析静默模式（每次打开都要新建窗口）耗时的构成。
//!
//! 后端记录收到打开请求、创建 WebView、开始显示等时间点；前端记录页面加载、首次渲染、
//! 列表就绪等时间点，内容就绪后通过 `report_open_timing` 一并交给后端，按时间顺序输出汇总。
//! 前端的时间点用系统时间（Unix 毫秒）表示，与后端记下的请求时刻对齐。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use specta::Type;

/// 前端报告的一个时间点。
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TimingMark {
    pub name: String,
    /// 系统时间，Unix 毫秒（`performance.timeOrigin + performance.now()`）。
    pub epoch_ms: f64,
}

/// 一次新建窗口打开过程中的时间点。
pub struct OpenTiming {
    requested_at: Instant,
    /// 收到打开请求时的系统时间（Unix 毫秒），用于换算前端的时间点。
    requested_epoch_ms: f64,
    /// 名称与距收到请求的毫秒数。
    marks: Vec<(String, f64)>,
}

impl OpenTiming {
    pub fn new(requested_at: Instant) -> Self {
        let now_epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        Self {
            requested_at,
            requested_epoch_ms: now_epoch_ms - millis(requested_at.elapsed()),
            marks: vec![("收到打开请求".into(), 0.0)],
        }
    }

    /// 记下后端的一个时间点（现在）。
    pub fn mark(&mut self, name: &str) {
        self.marks
            .push((name.into(), millis(self.requested_at.elapsed())));
    }

    /// 记下前端报告的时间点。
    pub fn mark_epoch(&mut self, mark: &TimingMark) {
        if mark.epoch_ms.is_finite() {
            self.marks
                .push((mark.name.clone(), mark.epoch_ms - self.requested_epoch_ms));
        }
    }

    /// 按时间顺序排列的汇总，每行为距收到请求的时间和与上一步的间隔。
    pub fn summary(&self) -> String {
        let mut marks = self.marks.clone();
        marks.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut text =
            String::from("[timing] 新建窗口分段耗时（距收到打开请求 / 与上一步的间隔）：");
        let mut previous = 0.0;
        for (name, at) in &marks {
            text.push_str(&format!(
                "\n  {at:>7.1} ms  {:>+7.1} ms  {name}",
                at - previous
            ));
            previous = *at;
        }
        text
    }
}

fn millis(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing() -> OpenTiming {
        OpenTiming {
            requested_at: Instant::now(),
            requested_epoch_ms: 1_000_000.0,
            marks: vec![("收到打开请求".into(), 0.0)],
        }
    }

    #[test]
    fn 前端时间点按请求时刻换算() {
        let mut t = timing();
        t.mark_epoch(&TimingMark {
            name: "首帧".into(),
            epoch_ms: 1_000_250.0,
        });
        assert_eq!(t.marks.last(), Some(&("首帧".into(), 250.0)));
    }

    #[test]
    fn 无效时间点被忽略() {
        let mut t = timing();
        t.mark_epoch(&TimingMark {
            name: "坏".into(),
            epoch_ms: f64::NAN,
        });
        assert_eq!(t.marks.len(), 1);
    }

    #[test]
    fn 汇总按时间排序并给出间隔() {
        let mut t = timing();
        t.marks.push(("后".into(), 300.0));
        t.marks.push(("前".into(), 100.0));
        let summary = t.summary();
        let lines: Vec<_> = summary.lines().skip(1).collect();
        assert!(lines[0].ends_with("收到打开请求"));
        assert!(
            lines[1].contains("100.0 ms")
                && lines[1].contains("+100.0 ms")
                && lines[1].ends_with("前")
        );
        assert!(
            lines[2].contains("300.0 ms")
                && lines[2].contains("+200.0 ms")
                && lines[2].ends_with("后")
        );
    }
}
