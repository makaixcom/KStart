//! 按用户设置读取网站图标，网络请求在窗口线程之外执行。

use crate::{skin_assets::RgbaSurface, to_wide};
use std::{
    collections::HashSet,
    ffi::c_void,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    mem,
    path::PathBuf,
    ptr,
    sync::{mpsc, Arc, Mutex, OnceLock},
};
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryHeaders, WinHttpQueryOption, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts, URL_COMPONENTS,
    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_INTERNET_SCHEME_HTTP,
    WINHTTP_INTERNET_SCHEME_HTTPS, WINHTTP_OPTION_REDIRECT_POLICY,
    WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    WINHTTP_OPTION_URL, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

const LIMIT: usize = 128 * 1024;
type ResultImage = (String, RgbaSurface);
static RESULTS: OnceLock<Mutex<Vec<ResultImage>>> = OnceLock::new();

#[derive(Clone, PartialEq, Eq, Hash)]
struct Request {
    root: PathBuf,
    url: String,
    cache: bool,
}

struct WorkerPool {
    sender: mpsc::Sender<Request>,
    pending: Arc<Mutex<HashSet<Request>>>,
}

impl WorkerPool {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Request>();
        let receiver = Arc::new(Mutex::new(receiver));
        let pending = Arc::new(Mutex::new(HashSet::new()));
        for _ in 0..2 {
            let receiver = Arc::clone(&receiver);
            let pending = Arc::clone(&pending);
            std::thread::spawn(move || loop {
                let request = match receiver.lock() {
                    Ok(queue) => match queue.recv() {
                        Ok(request) => request,
                        Err(_) => break,
                    },
                    Err(_) => break,
                };
                if let Some(icon) = fetch(&request.root, &request.url, request.cache) {
                    if let Ok(mut results) = RESULTS.get_or_init(|| Mutex::new(Vec::new())).lock() {
                        results.push((request.url.clone(), icon));
                    }
                }
                if let Ok(mut pending) = pending.lock() {
                    pending.remove(&request);
                }
            });
        }
        Self { sender, pending }
    }

    fn enqueue(&self, request: Request) {
        let Ok(mut pending) = self.pending.lock() else {
            return;
        };
        if !pending.insert(request.clone()) {
            return;
        }
        drop(pending);
        if let Err(error) = self.sender.send(request) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&error.0);
            }
        }
    }
}

static WORKERS: OnceLock<WorkerPool> = OnceLock::new();

#[cfg(test)]
mod worker_tests {
    use super::*;

    #[test]
    fn repeated_requests_are_coalesced_and_failed_requests_can_retry() {
        let (sender, receiver) = mpsc::channel();
        let pool = WorkerPool {
            sender,
            pending: Arc::new(Mutex::new(HashSet::new())),
        };
        let request = Request {
            root: "first".into(),
            url: "https://example.invalid/a".into(),
            cache: true,
        };
        pool.enqueue(request.clone());
        pool.enqueue(request.clone());
        let mut other = request.clone();
        other.root = "second".into();
        pool.enqueue(other);
        assert_eq!(receiver.try_iter().count(), 2);
        pool.pending.lock().unwrap().remove(&request);
        pool.enqueue(request);
        assert_eq!(receiver.try_iter().count(), 1);
    }
}

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
    WORKERS
        .get_or_init(WorkerPool::new)
        .enqueue(Request { root, url, cache });
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
    let icon = decode_icon(&data, 16)?;
    if cache && fs::create_dir_all(dir).is_ok() {
        let _ = icon.save_png(&path);
    }
    Some(icon)
}

pub(crate) fn decode_icon(data: &[u8], size: u32) -> Option<RgbaSurface> {
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
        .resize_exact(size, size, image::imageops::FilterType::Triangle)
        .to_rgba8();
    Some(RgbaSurface::from_pixels(
        size,
        size,
        image.pixels().map(|pixel| pixel.0).collect(),
    ))
}

/// 元信息请求与已有图标下载共用 WinHTTP、超时和长度限制。
pub(crate) fn download_url(url: &str, limit: usize) -> Option<(Vec<u8>, String)> {
    unsafe {
        let wide = to_wide(url);
        let mut info: URL_COMPONENTS = mem::zeroed();
        info.dwStructSize = mem::size_of::<URL_COMPONENTS>() as u32;
        info.dwHostNameLength = u32::MAX;
        info.dwUrlPathLength = u32::MAX;
        info.dwExtraInfoLength = u32::MAX;
        if WinHttpCrackUrl(wide.as_ptr(), 0, 0, &mut info) == 0
            || info.lpszHostName.is_null()
            || !matches!(
                info.nScheme,
                WINHTTP_INTERNET_SCHEME_HTTP | WINHTTP_INTERNET_SCHEME_HTTPS
            )
        {
            return None;
        }
        let part = |pointer: *const u16, length: u32| {
            if pointer.is_null() {
                String::new()
            } else {
                String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length as usize))
            }
        };
        let host = part(info.lpszHostName, info.dwHostNameLength);
        let mut path = part(info.lpszUrlPath, info.dwUrlPathLength);
        if path.is_empty() {
            path.push('/');
        }
        path.push_str(&part(info.lpszExtraInfo, info.dwExtraInfoLength));
        let path = path.split('#').next()?;
        download_resource(
            &host,
            info.nPort,
            info.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS,
            path,
            limit,
            true,
            url,
        )
    }
}

unsafe fn download(host: &str, port: u16, secure: bool) -> Option<Vec<u8>> {
    download_resource(host, port, secure, "/favicon.ico", LIMIT, false, "").map(|(data, _)| data)
}

unsafe fn download_resource(
    host: &str,
    port: u16,
    secure: bool,
    path: &str,
    limit: usize,
    redirects: bool,
    original: &str,
) -> Option<(Vec<u8>, String)> {
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
        to_wide(path).as_ptr(),
        ptr::null(),
        ptr::null(),
        ptr::null(),
        if secure { WINHTTP_FLAG_SECURE } else { 0 },
    ))?;
    let policy = if redirects {
        WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP
    } else {
        WINHTTP_OPTION_REDIRECT_POLICY_NEVER
    };
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
    let mut final_url = original.to_string();
    if redirects {
        let mut size = 0;
        WinHttpQueryOption(req.0, WINHTTP_OPTION_URL, ptr::null_mut(), &mut size);
        if size > 0 && size < 32 * 1024 {
            let mut value = vec![0u16; size as usize / 2 + 1];
            if WinHttpQueryOption(
                req.0,
                WINHTTP_OPTION_URL,
                value.as_mut_ptr().cast(),
                &mut size,
            ) != 0
            {
                let end = value.iter().position(|&ch| ch == 0).unwrap_or(value.len());
                final_url = String::from_utf16_lossy(&value[..end]);
            }
        }
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
            return Some((data, final_url));
        }
        if data.len() + count as usize > limit {
            return None;
        }
        data.extend_from_slice(&buffer[..count as usize]);
    }
}
