//! 按用户设置读取网站图标，网络请求在窗口线程之外执行。

use crate::{skin_assets::RgbaSurface, to_wide};
use std::{
    ffi::c_void,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    mem,
    path::PathBuf,
    ptr,
    sync::{Mutex, OnceLock},
};
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts, URL_COMPONENTS, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
    WINHTTP_FLAG_SECURE, WINHTTP_INTERNET_SCHEME_HTTP, WINHTTP_INTERNET_SCHEME_HTTPS,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

const LIMIT: usize = 128 * 1024;
type ResultImage = (String, RgbaSurface);
static RESULTS: OnceLock<Mutex<Vec<ResultImage>>> = OnceLock::new();

struct Handle(*mut c_void);

impl Handle {
    fn from(raw: *mut c_void) -> Option<Self> {
        (!raw.is_null()).then(|| Self(raw))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}

pub fn is_web_url(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    })
}

pub fn fetch_for_shortcut(root: PathBuf, url: String, cache: bool) {
    if !is_web_url(&url) {
        return;
    }
    std::thread::spawn(move || {
        if let Some(icon) = fetch(&root, &url, cache) {
            if let Ok(mut results) = RESULTS.get_or_init(|| Mutex::new(Vec::new())).lock() {
                results.push((url, icon));
            }
        }
    });
}

pub fn take_results() -> Vec<ResultImage> {
    RESULTS
        .get()
        .and_then(|queue| queue.try_lock().ok())
        .map(|mut queue| mem::take(&mut *queue))
        .unwrap_or_default()
}

fn fetch(root: &std::path::Path, url: &str, cache: bool) -> Option<RgbaSurface> {
    let wide = to_wide(url);
    let (host, port, secure) = unsafe {
        let mut info: URL_COMPONENTS = mem::zeroed();
        info.dwStructSize = mem::size_of::<URL_COMPONENTS>() as u32;
        info.dwHostNameLength = u32::MAX;
        if WinHttpCrackUrl(wide.as_ptr(), 0, 0, &mut info) == 0
            || info.lpszHostName.is_null()
            || !matches!(
                info.nScheme,
                WINHTTP_INTERNET_SCHEME_HTTP | WINHTTP_INTERNET_SCHEME_HTTPS
            )
        {
            return None;
        }
        let host = String::from_utf16_lossy(std::slice::from_raw_parts(
            info.lpszHostName,
            info.dwHostNameLength as usize,
        ));
        (
            host,
            info.nPort,
            info.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS,
        )
    };
    let mut hash = DefaultHasher::new();
    (&host, port, secure).hash(&mut hash);
    let dir = root.join("user/icon-cache");
    let path = dir.join(format!("web-{:016x}.png", hash.finish()));
    if cache {
        if let Some(icon) = RgbaSurface::load_file(&path) {
            return Some(icon);
        }
    }
    let data = unsafe { download(&host, port, secure)? };
    let mut reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(512);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .ok()?
        .resize_exact(16, 16, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let mut icon = RgbaSurface::new_empty(16, 16);
    for (dst, src) in icon.pixels.iter_mut().zip(image.pixels()) {
        *dst = src.0;
    }
    if cache && fs::create_dir_all(dir).is_ok() {
        let _ = icon.save_png(&path);
    }
    Some(icon)
}

unsafe fn download(host: &str, port: u16, secure: bool) -> Option<Vec<u8>> {
    let session = Handle::from(WinHttpOpen(
        to_wide("KStart").as_ptr(),
        WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
        ptr::null(),
        ptr::null(),
        0,
    ))?;
    WinHttpSetTimeouts(session.0, 1500, 1500, 1500, 1500);
    let server = Handle::from(WinHttpConnect(session.0, to_wide(host).as_ptr(), port, 0))?;
    let req = Handle::from(WinHttpOpenRequest(
        server.0,
        ptr::null(),
        to_wide("/favicon.ico").as_ptr(),
        ptr::null(),
        ptr::null(),
        ptr::null(),
        if secure { WINHTTP_FLAG_SECURE } else { 0 },
    ))?;
    let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
    if WinHttpSetOption(
        req.0,
        WINHTTP_OPTION_REDIRECT_POLICY,
        (&policy as *const u32).cast(),
        4,
    ) == 0
        || WinHttpSendRequest(req.0, ptr::null(), 0, ptr::null(), 0, 0, 0) == 0
        || WinHttpReceiveResponse(req.0, ptr::null_mut()) == 0
    {
        return None;
    }
    let mut status = 0u32;
    let mut length = 4;
    if WinHttpQueryHeaders(
        req.0,
        WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
        ptr::null(),
        (&mut status as *mut u32).cast(),
        &mut length,
        ptr::null_mut(),
    ) == 0
        || status != 200
    {
        return None;
    }
    let mut data = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let mut count = 0;
        if WinHttpReadData(
            req.0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut count,
        ) == 0
        {
            return None;
        }
        if count == 0 {
            return Some(data);
        }
        if data.len() + count as usize > LIMIT {
            return None;
        }
        data.extend_from_slice(&buffer[..count as usize]);
    }
}
