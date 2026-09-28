//! 进程内存与 CPU 时间，用于核对 MVP 验收标准中的资源占用。

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

pub fn report() -> String {
    let process = unsafe { GetCurrentProcess() };

    let mut memory = PROCESS_MEMORY_COUNTERS::default();
    let memory_ok = unsafe {
        GetProcessMemoryInfo(
            process,
            &mut memory,
            size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    }
    .is_ok();

    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    let times_ok =
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }
            .is_ok();

    let mut parts = Vec::new();
    if memory_ok {
        parts.push(format!(
            "工作集 {:.1} MB（峰值 {:.1} MB）",
            mb(memory.WorkingSetSize),
            mb(memory.PeakWorkingSetSize)
        ));
    }
    if times_ok {
        // 空闲时该值不应增长，否则说明存在轮询或忙等。
        parts.push(format!(
            "累计 CPU 时间 {:.3} 秒",
            seconds(&kernel) + seconds(&user)
        ));
    }
    if parts.is_empty() {
        "无法读取进程资源信息".into()
    } else {
        parts.join("，")
    }
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

/// FILETIME 以 100 纳秒为单位。
fn seconds(time: &FILETIME) -> f64 {
    let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    ticks as f64 / 10_000_000.0
}
