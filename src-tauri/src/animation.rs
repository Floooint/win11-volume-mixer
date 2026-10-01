//! 窗口动画：滑入 / 滑出、高度变化。在后台线程逐帧移动或缩放窗口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 动画帧率范围（帧 / 秒），与设置项 `animation_fps` 对应。
pub const FPS_RANGE: std::ops::RangeInclusive<u32> = 30..=240;

/// 读取不到显示器刷新率时使用的帧率。
const FALLBACK_FPS: u32 = 60;

/// 查询显示器当前刷新率（Hz）。`device` 为显示器设备名（如 `\\.\DISPLAY1`），
/// `None` 表示主显示器。读取失败或系统返回“默认值”时返回 `None`。
pub fn display_refresh_rate(device: Option<&str>) -> Option<u32> {
    use windows::Win32::Graphics::Gdi::{DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW};
    use windows_core::{HSTRING, PCWSTR};

    let name = device.map(HSTRING::from);
    let name_ptr = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: mode 已按要求设置 dmSize；name_ptr 为空或指向存活到调用结束的 HSTRING。
    let ok = unsafe { EnumDisplaySettingsW(name_ptr, ENUM_CURRENT_SETTINGS, &mut mode) };
    // 0 或 1 表示“硬件默认刷新率”，不是真实值。
    (ok.as_bool() && mode.dmDisplayFrequency > 1).then_some(mode.dmDisplayFrequency)
}

/// 实际使用的帧率：用户设置了就用设置值，否则跟随显示器刷新率。
pub fn effective_fps(configured: Option<u32>, refresh_rate: Option<u32>) -> u32 {
    configured
        .or(refresh_rate)
        .unwrap_or(FALLBACK_FPS)
        .clamp(*FPS_RANGE.start(), *FPS_RANGE.end())
}

/// 每帧间隔。实际位置按经过的时间计算，线程调度抖动只影响平滑度，不影响总时长。
fn frame_interval(fps: u32) -> Duration {
    let fps = fps.clamp(*FPS_RANGE.start(), *FPS_RANGE.end());
    Duration::from_secs_f64(1.0 / f64::from(fps))
}

/// 一段动画：时长与缓动曲线。
#[derive(Debug, Clone, Copy)]
pub struct Motion {
    pub duration: Duration,
    pub curve: CubicBezier,
}

impl Motion {
    /// 时长乘以 `factor`（动画速度），曲线不变。
    pub fn scaled(self, factor: f64) -> Self {
        Self {
            duration: self.duration.mul_f64(factor.max(0.0)),
            ..self
        }
    }

    /// 时长为零：不播放动画，直接到达终点。
    pub fn instant(self) -> Self {
        self.scaled(0.0)
    }
}

/// 进入：从任务栏一侧轻微滑入。ease-out cubic `cubic-bezier(0.33, 1, 0.68, 1)`，起步快、平稳停下。
pub const ENTER: Motion = Motion {
    duration: Duration::from_millis(50),
    curve: CubicBezier::new(0.33, 1.0, 0.68, 1.0),
};

/// 退出：向任务栏一侧滑出屏幕。ease-in cubic `cubic-bezier(0.32, 0, 0.67, 0)`，起步慢、逐渐加速。
pub const EXIT: Motion = Motion {
    duration: Duration::from_millis(100),
    curve: CubicBezier::new(0.32, 0.0, 0.67, 0.0),
};

/// 进入：从屏幕边缘整段滑入。移动距离比轻微滑入长得多，时长也相应加长。
pub const ENTER_FROM_EDGE: Motion = Motion {
    duration: Duration::from_millis(160),
    curve: CubicBezier::new(0.33, 1.0, 0.68, 1.0),
};

/// 高度变化（切换页面、应用增减）：ease-out cubic，与进入相同的曲线，时长更长以便看清变化。
pub const RESIZE: Motion = Motion {
    duration: Duration::from_millis(180),
    curve: CubicBezier::new(0.33, 1.0, 0.68, 1.0),
};

/// CSS 同款三次贝塞尔缓动曲线，端点固定为 (0,0) 和 (1,1)。
#[derive(Debug, Clone, Copy)]
pub struct CubicBezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl CubicBezier {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn sample(a1: f64, a2: f64, t: f64) -> f64 {
        // B(t) = 3(1-t)²t·a1 + 3(1-t)t²·a2 + t³
        let u = 1.0 - t;
        3.0 * u * u * t * a1 + 3.0 * u * t * t * a2 + t * t * t
    }

    /// 给定时间进度 `x`（0–1），返回位移进度。x 方向单调，用二分法求参数 t。
    pub fn ease(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            if Self::sample(self.x1, self.x2, mid) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Self::sample(self.y1, self.y2, (lo + hi) / 2.0)
    }
}

/// 动画代数：每次开始或取消动画都加一，旧动画发现代数变化后立即停止。
#[derive(Default)]
pub struct Generation(AtomicU64);

impl Generation {
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.0.load(Ordering::SeqCst) == generation
    }
}

/// 按 `motion` 以 `fps` 帧率播放动画，每帧以缓动后的进度（0–1）调用 `frame`，最后一帧为 1。
/// 返回 `true` 表示完整播放，`false` 表示中途被取消。
pub fn run(
    motion: Motion,
    fps: u32,
    still_current: impl Fn() -> bool,
    frame: impl Fn(f64),
) -> bool {
    if motion.duration.is_zero() {
        // 时长为零时进度是 0/0，直接给出最后一帧。
        if !still_current() {
            return false;
        }
        frame(1.0);
        return true;
    }
    let interval = frame_interval(fps);
    let start = Instant::now();
    loop {
        if !still_current() {
            return false;
        }
        let progress = start.elapsed().as_secs_f64() / motion.duration.as_secs_f64();
        frame(motion.curve.ease(progress));
        if progress >= 1.0 {
            return true;
        }
        std::thread::sleep(interval);
    }
}

/// 从 `from` 到 `to` 按进度插值并取整。
pub fn lerp(from: i32, to: i32, progress: f64) -> i32 {
    (f64::from(from) + f64::from(to - from) * progress).round() as i32
}

/// 按 `motion` 以 `fps` 帧率从 `from` 移动到 `to`（`(x, y)`），每帧调用 `move_to(x, y)`。
/// 返回 `true` 表示完整播放，`false` 表示中途被取消。
pub fn slide(
    motion: Motion,
    fps: u32,
    from: (i32, i32),
    to: (i32, i32),
    still_current: impl Fn() -> bool,
    move_to: impl Fn(i32, i32),
) -> bool {
    run(motion, fps, still_current, |progress| {
        move_to(lerp(from.0, to.0, progress), lerp(from.1, to.1, progress))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn 曲线端点为0和1() {
        for motion in [ENTER, ENTER_FROM_EDGE, EXIT, RESIZE] {
            assert!(motion.curve.ease(0.0).abs() < 1e-9);
            assert!((motion.curve.ease(1.0) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn 超出范围的进度被截断() {
        assert!(EXIT.curve.ease(-1.0).abs() < 1e-9);
        assert!((EXIT.curve.ease(2.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn 退出曲线先慢后快() {
        let mut previous = 0.0;
        for i in 1..=100 {
            let x = f64::from(i) / 100.0;
            let y = EXIT.curve.ease(x);
            assert!(y >= previous, "x={x} 处不单调");
            if x < 1.0 {
                assert!(y < x, "x={x} 处应慢于匀速");
            }
            previous = y;
        }
        assert!(EXIT.curve.ease(0.5) < 0.15, "前半段只走完不到 15%");
    }

    #[test]
    fn 进入曲线先快后慢() {
        let mut previous = 0.0;
        for i in 1..=100 {
            let x = f64::from(i) / 100.0;
            let y = ENTER.curve.ease(x);
            assert!(y >= previous, "x={x} 处不单调");
            if x < 1.0 {
                assert!(y > x, "x={x} 处应快于匀速");
            }
            previous = y;
        }
        assert!(ENTER.curve.ease(0.5) > 0.85, "前半段已走完 85% 以上");
    }

    #[test]
    fn 时长符合要求() {
        assert_eq!(ENTER.duration, Duration::from_millis(50));
        assert_eq!(EXIT.duration, Duration::from_millis(100));
    }

    #[test]
    fn 线性曲线等于匀速() {
        let linear = CubicBezier::new(0.0, 0.0, 1.0, 1.0);
        for i in 0..=10 {
            let x = f64::from(i) / 10.0;
            assert!((linear.ease(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn 动画结束于目标位置() {
        for (motion, from, to) in [(EXIT, 100, 500), (ENTER, 500, 100)] {
            let positions = RefCell::new(Vec::new());
            let completed = slide(
                motion,
                90,
                (0, from),
                (0, to),
                || true,
                |_, y| positions.borrow_mut().push(y),
            );
            assert!(completed);
            let positions = positions.into_inner();
            assert_eq!(positions.first(), Some(&from));
            assert_eq!(positions.last(), Some(&to));
        }
    }

    #[test]
    fn 取消后立即停止() {
        let frames = RefCell::new(0);
        let completed = slide(
            EXIT,
            90,
            (0, 0),
            (0, 100),
            || *frames.borrow() < 2,
            |_, _| *frames.borrow_mut() += 1,
        );
        assert!(!completed);
        assert_eq!(*frames.borrow(), 2);
    }

    #[test]
    fn 未设置帧率时跟随显示器刷新率() {
        assert_eq!(effective_fps(None, Some(144)), 144);
        assert_eq!(effective_fps(None, Some(60)), 60);
    }

    #[test]
    fn 设置了帧率时优先使用设置值() {
        assert_eq!(effective_fps(Some(90), Some(144)), 90);
    }

    #[test]
    fn 读不到刷新率时使用60帧() {
        assert_eq!(effective_fps(None, None), 60);
    }

    #[test]
    fn 实际帧率被限制在范围内() {
        assert_eq!(effective_fps(None, Some(360)), 240, "360Hz 显示器按 240 帧");
        assert_eq!(effective_fps(None, Some(24)), 30);
    }

    #[test]
    fn 能读取主显示器刷新率() {
        // 在有显示器的开发机上运行；读取成功时值应在合理范围内。
        if let Some(hz) = display_refresh_rate(None) {
            assert!((20..=1000).contains(&hz), "刷新率 {hz} 不合理");
        }
    }

    #[test]
    fn 帧间隔随帧率变化并被限制在范围内() {
        assert_eq!(frame_interval(90), Duration::from_secs_f64(1.0 / 90.0));
        assert_eq!(frame_interval(1), frame_interval(30), "低于下限按 30 帧");
        assert_eq!(
            frame_interval(1000),
            frame_interval(240),
            "高于上限按 240 帧"
        );
    }

    #[test]
    fn 帧率越高帧数越多() {
        let count = |fps| {
            let frames = RefCell::new(0);
            slide(
                EXIT,
                fps,
                (0, 0),
                (0, 100),
                || true,
                |_, _| *frames.borrow_mut() += 1,
            );
            frames.into_inner()
        };
        let (low, high) = (count(30), count(240));
        assert!(high > low, "240 帧（{high}）应多于 30 帧（{low}）");
    }

    #[test]
    fn 插值取整且端点准确() {
        assert_eq!(lerp(100, 300, 0.0), 100);
        assert_eq!(lerp(100, 300, 0.5), 200);
        assert_eq!(lerp(300, 100, 1.0), 100);
        assert_eq!(lerp(0, 3, 0.5), 2, "1.5 四舍五入为 2");
    }

    #[test]
    fn 水平方向滑动结束于目标位置() {
        let positions = RefCell::new(Vec::new());
        slide(
            ENTER,
            90,
            (-50, 300),
            (100, 300),
            || true,
            |x, y| positions.borrow_mut().push((x, y)),
        );
        let positions = positions.into_inner();
        assert_eq!(positions.first(), Some(&(-50, 300)));
        assert_eq!(positions.last(), Some(&(100, 300)));
        assert!(positions.iter().all(|&(_, y)| y == 300), "纵向不动");
    }

    #[test]
    fn 无动画时只有一帧且立即到达终点() {
        let positions = RefCell::new(Vec::new());
        let completed = slide(
            EXIT.instant(),
            90,
            (0, 0),
            (0, 100),
            || true,
            |_, y| positions.borrow_mut().push(y),
        );
        assert!(completed);
        assert_eq!(positions.into_inner(), [100]);
    }

    #[test]
    fn 速度按倍数缩放时长() {
        assert_eq!(EXIT.scaled(2.0).duration, Duration::from_millis(200));
        assert_eq!(EXIT.scaled(0.5).duration, Duration::from_millis(50));
        assert_eq!(EXIT.scaled(-1.0).duration, Duration::ZERO, "负数按零");
    }

    #[test]
    fn 代数变化后旧动画失效() {
        let generation = Generation::default();
        let first = generation.next();
        assert!(generation.is_current(first));
        let second = generation.next();
        assert!(!generation.is_current(first));
        assert!(generation.is_current(second));
    }
}
