//! 读取和写入 Windows 快捷方式。

use crate::config::{decode_text_bytes, parse_ini_map};
use crate::to_wide;
use std::{
    ffi::OsStr,
    fs, io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
};
use windows::core::{Interface, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_Programs, SHGetKnownFolderPath};

pub(crate) struct Com;

impl Com {
    pub(crate) fn initialize() -> windows::core::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn read_string(buffer: &[u16]) -> String {
    String::from_utf16_lossy(&buffer[..buffer.iter().position(|&u| u == 0).unwrap_or(buffer.len())])
}

pub fn file_link(path: &Path) -> Option<(String, String, String)> {
    let _com = Com::initialize().ok()?;
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        persist
            .Load(PCWSTR(wide_path(path).as_ptr()), STGM_READ)
            .ok()?;
        let mut target = [0u16; 1024];
        let mut args = [0u16; 1024];
        let mut directory = [0u16; 1024];
        link.GetPath(&mut target, ptr::null_mut(), 0).ok()?;
        if target[0] == 0 {
            return None;
        }
        let _ = link.GetArguments(&mut args);
        let _ = link.GetWorkingDirectory(&mut directory);
        Some((
            read_string(&target),
            read_string(&args),
            read_string(&directory),
        ))
    }
}

pub fn url_link(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    parse_ini_map(&decode_text_bytes(&bytes))
        .remove("URL")
        .filter(|url| !url.is_empty())
}

/// 导出可由 Windows 独立运行的文件或目录快捷方式。
pub fn write_file_link(output: &Path, target: &str, args: &str, directory: &str, icon: &str, remark: &str)
    -> Result<(), Box<dyn std::error::Error>> {
    let _com = Com::initialize()?;
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(PCWSTR(to_wide(target).as_ptr()))?;
        link.SetArguments(PCWSTR(to_wide(args).as_ptr()))?;
        link.SetWorkingDirectory(PCWSTR(to_wide(directory).as_ptr()))?;
        link.SetDescription(PCWSTR(to_wide(remark).as_ptr()))?;
        if !icon.is_empty() && Path::new(icon).is_file() { link.SetIconLocation(PCWSTR(to_wide(icon).as_ptr()), 0)?; }
        let persist: IPersistFile = link.cast()?;
        persist.Save(PCWSTR(wide_path(output).as_ptr()), true)?;
    }
    Ok(())
}

pub fn install_shortcut(root: &Path, desktop: bool) -> Result<(), Box<dyn std::error::Error>> {
    let _com = Com::initialize()?;
    let exe = std::env::current_exe()?;
    unsafe {
        let mut path = ptr::null_mut();
        let folder = if desktop {
            &FOLDERID_Desktop
        } else {
            &FOLDERID_Programs
        };
        if SHGetKnownFolderPath(folder, 0, 0, &mut path) < 0 || path.is_null() {
            return Err(io::Error::other("找不到系统快捷方式目录").into());
        }
        let mut length = 0;
        while *path.add(length) != 0 {
            length += 1;
        }
        use std::os::windows::ffi::OsStringExt;
        let dir = std::ffi::OsString::from_wide(std::slice::from_raw_parts(path, length));
        CoTaskMemFree(path.cast());
        let output = PathBuf::from(dir).join("KStart.lnk");
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(PCWSTR(wide_path(&exe).as_ptr()))?;
        link.SetWorkingDirectory(PCWSTR(wide_path(root).as_ptr()))?;
        let args = OsStr::new("--base-dir ")
            .encode_wide()
            .chain(to_wide(&format!("\"{}\"", root.display())))
            .collect::<Vec<_>>();
        link.SetArguments(PCWSTR(args.as_ptr()))?;
        let persist: IPersistFile = link.cast()?;
        persist.Save(PCWSTR(wide_path(&output).as_ptr()), true)?;
    }
    Ok(())
}
