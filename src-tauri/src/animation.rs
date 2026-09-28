//! 窗口滑入 / 滑出动画：在后台线程逐帧移动窗口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 动画帧率范围（帧 / 秒），与设置项 `animation_fps` 对应。
pub const FPS_RANGE: std::ops::RangeInclusive<u32> = 30..=240;

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

/// 进入：从下方滑入。ease-out cubic `cubic-bezier(0.33, 1, 0.68, 1)`，起步快、平稳停下。
pub const ENTER: Motion = Motion {
    duration: Duration::from_millis(50),
    curve: CubicBezier::new(0.33, 1.0, 0.68, 1.0),
};

/// 退出：向下滑出。ease-in cubic `cubic-bezier(0.32, 0, 0.67, 0)`，起步慢、逐渐加速。
pub const EXIT: Motion = Motion {
    duration: Duration::from_millis(100),
    curve: CubicBezier::new(0.32, 0.0, 0.67, 0.0),
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

/// 按 `motion` 以 `fps` 帧率从 `from_y` 移动到 `to_y`，每帧调用 `move_to(y)`。
/// 返回 `true` 表示完整播放，`false` 表示中途被取消。
pub fn slide(
    motion: Motion,
    fps: u32,
    from_y: i32,
    to_y: i32,
    still_current: impl Fn() -> bool,
    move_to: impl Fn(i32),
) -> bool {
    let frame = frame_interval(fps);
    let start = Instant::now();
    loop {
        if !still_current() {
            return false;
        }
        let progress = start.elapsed().as_secs_f64() / motion.duration.as_secs_f64();
        let eased = motion.curve.ease(progress);
        let y = from_y as f64 + (to_y - from_y) as f64 * eased;
        move_to(y.round() as i32);
        if progress >= 1.0 {
            return true;
        }
        std::thread::sleep(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn 曲线端点为0和1() {
        for motion in [ENTER, EXIT] {
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
                from,
                to,
                || true,
                |y| positions.borrow_mut().push(y),
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
            0,
            100,
            || *frames.borrow() < 2,
            |_| *frames.borrow_mut() += 1,
        );
        assert!(!completed);
        assert_eq!(*frames.borrow(), 2);
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
            slide(EXIT, fps, 0, 100, || true, |_| *frames.borrow_mut() += 1);
            frames.into_inner()
        };
        let (low, high) = (count(30), count(240));
        assert!(high > low, "240 帧（{high}）应多于 30 帧（{low}）");
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
