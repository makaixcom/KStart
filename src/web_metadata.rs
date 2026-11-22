//! 超链接标题和 favicon；仅在后台线程调用网络入口。
use crate::{
    advanced_web_icon::{decode_icon, download_url},
    skin_assets::RgbaSurface,
};

#[derive(Default)]
pub struct Metadata {
    pub title: Option<String>,
    pub icon: Option<RgbaSurface>,
}

pub fn normalize(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() || input.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let value = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let (scheme, rest) = value.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains(['@', '\\', '<', '>', '"', '\'']) {
        return None;
    }
    let host = if authority.starts_with('[') {
        let end = authority.find(']')?;
        authority[1..end].parse::<std::net::Ipv6Addr>().ok()?;
        let port = &authority[end + 1..];
        if !port.is_empty() {
            port.strip_prefix(':')?.parse::<u16>().ok()?;
        }
        &authority[..=end]
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        port.parse::<u16>().ok()?;
        host
    } else {
        authority
    };
    if host.is_empty() || host.contains(':') && !host.starts_with('[') {
        return None;
    }
    Some(value)
}

fn entities(input: &str) -> String {
    let mut result = String::new();
    let mut rest = input;
    while let Some(start) = rest.find('&') {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|&end| end < 16) else {
            result.push('&');
            rest = &rest[1..];
            continue;
        };
        let code = &rest[1..end];
        let ch = match code {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if code.starts_with("#x") || code.starts_with("#X") => {
                u32::from_str_radix(&code[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if code.starts_with('#') => code[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        if let Some(ch) = ch {
            result.push(ch);
        } else {
            result.push_str(&rest[..=end]);
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}

fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let bytes = tag.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        if i == start {
            i += 1;
            continue;
        }
        let key = tag[start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = bytes.get(i).copied().filter(|b| *b == b'\'' || *b == b'"');
        if quote.is_some() {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && if let Some(q) = quote {
                bytes[i] != q
            } else {
                !bytes[i].is_ascii_whitespace()
            }
        {
            i += 1;
        }
        result.push((key, entities(&tag[start..i])));
        if quote.is_some() {
            i += 1;
        }
    }
    result
}

/// 找标签终点时跳过引号中的 >，避免截断带查询参数的图标地址。
fn tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let lower = html.to_ascii_lowercase();
    let needle = format!("<{name}");
    let mut result = Vec::new();
    let mut offset = 0;
    while let Some(start) = lower[offset..].find(&needle) {
        let start = offset + start + needle.len();
        let bytes = html.as_bytes();
        if !bytes
            .get(start)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
        {
            offset = start;
            continue;
        }
        let mut end = start;
        let mut quote = None;
        while end < bytes.len() {
            let b = bytes[end];
            if quote == Some(b) {
                quote = None;
            } else if quote.is_none() && (b == b'"' || b == b'\'') {
                quote = Some(b);
            } else if quote.is_none() && b == b'>' {
                break;
            }
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }
        result.push(&html[start..end]);
        offset = end + 1;
    }
    result
}

fn resolve(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return None;
    }
    if href.contains("://") {
        return normalize(href);
    }
    let (scheme, rest) = base.split_once("://")?;
    if href.starts_with("//") {
        return normalize(&format!("{scheme}:{href}"));
    }
    // 禁止把 data:、javascript: 等解释成相对路径。
    if href.split('/').next()?.contains(':') {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?;
    let origin = format!("{scheme}://{host}");
    if href.starts_with('?') {
        return normalize(&format!("{}{href}", base.split(['?', '#']).next()?));
    }
    let path = if href.starts_with('/') {
        href.to_string()
    } else {
        let path = rest
            .get(host.len()..)
            .unwrap_or("/")
            .split(['?', '#'])
            .next()?;
        let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        format!("{directory}/{href}")
    };
    let end = path.find(['?', '#']).unwrap_or(path.len());
    let pathname = &path[..end];
    let suffix = &path[end..];
    let mut parts = Vec::new();
    for part in pathname.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let tail = if pathname.ends_with('/') && !parts.is_empty() {
        "/"
    } else {
        ""
    };
    normalize(&format!("{origin}/{}{tail}{suffix}", parts.join("/")))
}

pub fn parse(html: &str, url: &str) -> (Option<String>, Vec<String>) {
    let lower = html.to_ascii_lowercase();
    let title = lower.find("<title").and_then(|start| {
        let start = start + html[start..].find('>')? + 1;
        let end = start + lower[start..].find("</title")?;
        let text = entities(&html[start..end])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        (!text.is_empty()).then(|| text.chars().take(256).collect())
    });
    let base = tags(html, "base")
        .first()
        .and_then(|tag| attributes(tag).into_iter().find(|(key, _)| key == "href"))
        .and_then(|(_, href)| resolve(url, &href))
        .unwrap_or_else(|| url.to_string());
    let mut icons = Vec::new();
    let mut apple = Vec::new();
    for tag in tags(html, "link") {
        let attrs = attributes(tag);
        let get = |name| {
            attrs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        if let (Some(rel), Some(href)) = (get("rel"), get("href")) {
            let rel = rel.to_ascii_lowercase();
            if let Some(icon) = resolve(&base, href) {
                if rel.split_whitespace().any(|part| part == "icon") {
                    icons.push(icon);
                } else if rel
                    .split_whitespace()
                    .any(|part| part == "apple-touch-icon")
                {
                    apple.push(icon);
                }
            }
        }
    }
    icons.extend(apple);
    if let Some(root) = resolve(url, "/favicon.ico") {
        icons.push(root);
    }
    let mut unique = std::collections::HashSet::new();
    icons.retain(|icon| unique.insert(icon.clone()));
    (title, icons)
}

pub fn fetch(url: &str) -> Metadata {
    let (title, icons) = if let Some((bytes, final_url)) = download_url(url, 1024 * 1024) {
        parse(&crate::config::decode_text_bytes(&bytes), &final_url)
    } else {
        (None, resolve(url, "/favicon.ico").into_iter().collect())
    };
    let icon = icons.into_iter().take(3).find_map(|url| {
        download_url(&url, 128 * 1024).and_then(|(data, _)| decode_icon(&data, 32))
    });
    Metadata { title, icon }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_handles_entities_relative_icons_and_redirect_base() {
        let (title, icons) = parse(
            r#"<TITLE> 中文 &amp; &#x6807;题 </TITLE><base href='/assets/'><LINK href='fav.png?v=1&amp;x=2' rel='shortcut icon'><link rel=apple-touch-icon href=touch.png>"#,
            "https://example.com/moved/page",
        );
        assert_eq!(title.as_deref(), Some("中文 & 标题"));
        assert_eq!(
            icons,
            [
                "https://example.com/assets/fav.png?v=1&x=2",
                "https://example.com/assets/touch.png",
                "https://example.com/favicon.ico"
            ]
        );
    }
    #[test]
    fn urls_and_missing_metadata_have_predictable_fallbacks() {
        assert_eq!(
            normalize(" example.com ").as_deref(),
            Some("https://example.com")
        );
        assert!(normalize("https://").is_none());
        assert!(normalize("file://local").is_none());
        assert!(normalize("javascript:alert(1)").is_none());
        assert!(normalize("https://example.com:wrong").is_none());
        assert!(normalize("http://[::1]:8080/page").is_some());
        assert_eq!(
            parse("<html>no title</html>", "http://localhost:8080/a").1,
            ["http://localhost:8080/favicon.ico"]
        );
        assert_eq!(
            resolve("https://example.com/a/b", "../fav.ico").as_deref(),
            Some("https://example.com/fav.ico")
        );
        assert!(resolve("https://example.com", "data:image/png;base64,abc").is_none());
    }
}
