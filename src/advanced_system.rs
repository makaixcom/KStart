//! 由用户快捷方式触发的系统动作。

use crate::{
    advanced::{AdvancedSettings, Key},
    advanced_runtime::{confirm, message},
    to_wide,
};
use std::{
    ptr,
    time::{Duration, Instant},
};
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Media::Multimedia::mciSendStringW;
use windows_sys::Win32::System::Power::SetSuspendState;
use windows_sys::Win32::System::Shutdown::LockWorkStation;
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PostMessageW, HWND_BROADCAST, SC_MONITORPOWER, WM_SYSCOMMAND,
};

pub const ACTIONS: &[(&str, &str)] = &[
    ("控制面板", "control.exe"),
    ("关机", "kstart:shutdown"),
    ("重启", "kstart:restart"),
    ("注销", "kstart:logoff"),
    ("锁定计算机", "kstart:lock"),
    ("睡眠", "kstart:sleep"),
    ("休眠", "kstart:hibernate"),
    ("断开 Wi-Fi", "kstart:disconnect"),
    ("关闭显示器", "kstart:display-off"),
    ("启动屏幕保护", "kstart:screensaver"),
    ("弹出光驱", "kstart:cd-open"),
    ("关闭光驱", "kstart:cd-close"),
    ("增大音量", "kstart:volume-up"),
    ("减小音量", "kstart:volume-down"),
    ("静音/取消静音", "kstart:volume-mute"),
    ("清空回收站", "kstart:recycle"),
];

pub fn execute(hwnd: HWND, name: &str, settings: &AdvancedSettings) -> bool {
    let power = matches!(
        name,
        "shutdown" | "restart" | "logoff" | "hibernate" | "sleep"
    );
    if power && settings.enabled(Key::ConfirmPower)
        || name == "disconnect" && settings.enabled(Key::ConfirmDisconnect)
    {
        let action = ACTIONS
            .iter()
            .find(|(_, target)| target.strip_prefix("kstart:") == Some(name))
            .map_or("运行此系统功能", |(label, _)| *label);
        if !confirm(hwnd, &format!("确定要{action}吗？")) {
            return false;
        }
    }
    let ok = unsafe {
        match name {
            "shutdown" => crate::launch_target_url_or_path("shutdown.exe", "/s /t 0", "", hwnd),
            "restart" => crate::launch_target_url_or_path("shutdown.exe", "/r /t 0", "", hwnd),
            "logoff" => crate::launch_target_url_or_path("shutdown.exe", "/l", "", hwnd),
            "hibernate" => SetSuspendState(1, 0, 0) != 0,
            "sleep" => SetSuspendState(0, 0, 0) != 0,
            "lock" => LockWorkStation() != 0,
            "disconnect" => {
                crate::launch_target_url_or_path("netsh.exe", "wlan disconnect", "", hwnd)
            }
            "display-off" | "screensaver" => {
                if settings.enabled(Key::LockWithDisplayOff) {
                    LockWorkStation();
                }
                let (command, param) = if name == "display-off" {
                    (SC_MONITORPOWER, 2)
                } else {
                    (0xf140, 0)
                };
                PostMessageW(HWND_BROADCAST, WM_SYSCOMMAND, command as usize, param) != 0
            }
            "cd-open" => open_drive(settings.number(Key::CdCloseDelay)),
            "cd-close" => {
                mciSendStringW(
                    to_wide("set cdaudio door closed").as_ptr(),
                    ptr::null_mut(),
                    0,
                    0,
                ) == 0
            }
            "volume-mute" => mute_volume().is_ok(),
            "volume-up" | "volume-down" => {
                let step = settings.number(Key::VolumeStep) as f32 / 100.0;
                if let Ok(level) = change_volume(if name == "volume-up" { step } else { -step }) {
                    if !settings.enabled(Key::HideVolumeOverlay) {
                        crate::APP_STATE.with(|cell| {
                            if let Ok(mut guard) = cell.try_borrow_mut() {
                                if let Some(app) = guard.as_mut() {
                                    app.advanced_window.volume_indicator =
                                        Some((level, Instant::now()));
                                }
                            }
                        });
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    }
                    true
                } else {
                    false
                }
            }
            "recycle" => {
                let mut flags = 0;
                if settings.enabled(Key::RecycleNoConfirm) {
                    flags |= SHERB_NOCONFIRMATION;
                }
                if settings.enabled(Key::RecycleNoProgress) {
                    flags |= SHERB_NOPROGRESSUI;
                }
                if settings.enabled(Key::RecycleSilent) {
                    flags |= SHERB_NOSOUND;
                }
                let status = SHEmptyRecycleBinW(hwnd, ptr::null(), flags);
                status >= 0 || status as u32 == 0x800704c7
            }
            _ => false,
        }
    };
    if !ok {
        message(hwnd, "系统未能完成这个操作，请检查设备是否可用。");
    }
    ok
}

unsafe fn change_volume(delta: f32) -> windows::core::Result<u8> {
    let _com = crate::advanced_shell::Com::initialize()?;
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
    let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
    let level = (volume.GetMasterVolumeLevelScalar()? + delta).clamp(0.0, 1.0);
    volume.SetMasterVolumeLevelScalar(level, ptr::null())?;
    Ok((level * 100.0).round() as u8)
}

unsafe fn mute_volume() -> windows::core::Result<()> {
    let _com = crate::advanced_shell::Com::initialize()?;
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
    let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
    volume.SetMute(!volume.GetMute()?.as_bool(), ptr::null())
}

unsafe fn open_drive(seconds: u32) -> bool {
    if mciSendStringW(
        to_wide("set cdaudio door open").as_ptr(),
        ptr::null_mut(),
        0,
        0,
    ) != 0
    {
        return false;
    }
    if seconds != 0 {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(u64::from(seconds)));
            mciSendStringW(
                to_wide("set cdaudio door closed").as_ptr(),
                ptr::null_mut(),
                0,
                0,
            );
        });
    }
    true
}
