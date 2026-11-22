//! 系统功能与随程序分发的图标文件映射，列表和快捷方式共用。
use std::path::{Path, PathBuf};

pub fn relative_path(target: &str, args: &str) -> &'static str {
    match (target, args) {
        ("shell:MyComputerFolder", _) => "icons/system/sysdm.ico",
        ("shell:Personal", _) => "icons/system/MyDocument.ico",
        ("shell:RecycleBinFolder" | "kstart:recycle", _) => "icons/system/Recycle.ico",
        ("shell:NetworkPlacesFolder", _) | ("control.exe", "ncpa.cpl") => {
            "icons/system/NetWork.ico"
        }
        ("shell:PrintersFolder", _) => "icons/system/Printer.ico",
        ("control.exe", "") | ("kstart:app-advanced", _) => "icons/system/ControlPanel.ico",
        ("control.exe", "mmsys.cpl") | ("kstart:volume-up" | "kstart:volume-down", _) => {
            "icons/system/MMSYS.ico"
        }
        ("control.exe", "appwiz.cpl") => "icons/system/appwiz.ico",
        ("control.exe", "timedate.cpl") => "icons/system/timedate.ico",
        ("control.exe", "main.cpl") => "icons/system/mouse.ico",
        ("control.exe", "powercfg.cpl") => "icons/system/powercfg.ico",
        ("kstart:shutdown", _) => "icons/system/ShutDown.ico",
        ("kstart:restart", _) => "icons/system/ReBoot.ico",
        ("kstart:logoff", _) => "icons/system/LogOut.ico",
        ("kstart:lock", _) => "icons/system/LockMe.ico",
        ("kstart:sleep" | "kstart:hibernate", _) => "icons/system/Suspend.ico",
        ("kstart:disconnect", _) => "icons/system/Hangup.ico",
        ("kstart:screensaver", _) => "icons/system/ScreenSavers.ico",
        ("kstart:cd-open" | "kstart:cd-close", _) => "icons/system/CDRom.ico",
        ("kstart:display-off", _) => "icons/system/Monitor.ico",
        ("kstart:volume-mute", _) => "icons/system/Mute.ico",
        ("rasphone.exe", _) => "icons/system/Rasphone.ico",
        ("kstart:app-show" | "/AH/", _) => "icons/system/TaskBar.ico",
        ("kstart:app-hide", _) => "icons/system/DeskTop.ico",
        ("kstart:app-exit", _) => "icons/system/KillProgram.ico",
        ("kstart:app-hotkeys", _) => "icons/system/keyboard.ico",
        _ => "icons/system/Program.ico",
    }
}

pub fn path(base: &Path, target: &str, args: &str) -> PathBuf {
    base.join(relative_path(target, args))
}
