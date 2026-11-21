//! 使用 Windows 当前时区读取本地时间，日期格式化保持由 chrono 负责。
use chrono::{DateTime, FixedOffset, Utc};
use std::time::SystemTime;
use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

pub fn now() -> DateTime<FixedOffset> {
    let mut zone: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let state = unsafe { GetTimeZoneInformation(&mut zone) };
    // Win32 返回 1 表示标准时间，2 表示夏令时；偏置的符号与 UTC 偏移相反。
    let extra = match state {
        1 => zone.StandardBias,
        2 => zone.DaylightBias,
        _ => 0,
    };
    let seconds = if state == u32::MAX {
        0
    } else {
        -(zone.Bias + extra) * 60
    };
    let offset =
        FixedOffset::east_opt(seconds).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    DateTime::<Utc>::from(SystemTime::now()).with_timezone(&offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn native_local_clock_matches_windows_calendar() {
        unsafe fn sample() -> (i32, u32, u32, u32, u32) {
            let mut value = std::mem::zeroed();
            windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut value);
            (
                i32::from(value.wYear),
                u32::from(value.wMonth),
                u32::from(value.wDay),
                u32::from(value.wHour),
                u32::from(value.wMinute),
            )
        }
        let before = unsafe { sample() };
        let now = now();
        let actual = (now.year(), now.month(), now.day(), now.hour(), now.minute());
        let after = unsafe { sample() };
        assert!(actual == before || actual == after);
        assert!(!now.format("%Y-%m-%d %H:%M:%S %z").to_string().is_empty());
    }
}
