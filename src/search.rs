//! 搜索模式、快捷方式匹配及搜索输入的解释。

use crate::model::{CategoryTab, ShortcutItem};
use crate::skin_assets::RgbaSurface;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    Run,
    #[default]
    Shortcuts,
    Baidu,
    Google,
}

impl SearchMode {
    pub const OPTIONS: [(Self, &'static str); 4] = [
        (Self::Run, "运行(&R)"),
        (Self::Shortcuts, "搜索(&S)"),
        (Self::Baidu, "百度(&B)"),
        (Self::Google, "Google(&G)"),
    ];

    pub fn config_value(self) -> &'static str {
        match self {
            Self::Run => "Run",
            Self::Shortcuts => "Shortcuts",
            Self::Baidu => "Baidu",
            Self::Google => "Google",
        }
    }

    pub fn from_config(value: &str) -> Self {
        Self::OPTIONS
            .iter()
            .map(|&(mode, _)| mode)
            .find(|mode| mode.config_value().eq_ignore_ascii_case(value))
            .unwrap_or_default()
    }

    pub fn web_url(self, query: &str) -> Option<String> {
        let base = match self {
            Self::Baidu => "https://www.baidu.com/s?wd=",
            Self::Google => "https://www.google.com/search?q=",
            _ => return None,
        };
        let mut url = base.to_string();
        for byte in query.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                url.push(char::from(byte));
            } else {
                let _ = write!(url, "%{byte:02X}");
            }
        }
        Some(url)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchScope {
    Group,
    Category,
    All,
}

#[derive(Debug, Clone)]
pub struct SearchResultRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub icon: Option<RgbaSurface>,
}

#[derive(Debug, Clone)]
pub struct SearchDialogState {
    pub initial_query: String,
    pub results: Vec<SearchResultRow>,
}

impl SearchDialogState {
    pub fn new(initial_query: String) -> Self {
        Self {
            initial_query,
            results: Vec::new(),
        }
    }
}

pub fn matching_shortcuts<'a>(
    categories: &'a [CategoryTab],
    active_category: usize,
    query: &str,
    scope: SearchScope,
    fuzzy: bool,
) -> Vec<&'a ShortcutItem> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for (category_index, category) in categories.iter().enumerate() {
        if scope != SearchScope::All && category_index != active_category {
            continue;
        }
        let active_group = category
            .active_group_index
            .min(category.groups.len().saturating_sub(1));
        for (group_index, group) in category.groups.iter().enumerate() {
            if scope == SearchScope::Group && group_index != active_group {
                continue;
            }
            for item in &group.items {
                let matches = |text: &str| {
                    let text = text.to_lowercase();
                    if fuzzy {
                        text.contains(&query)
                    } else {
                        text == query
                    }
                };
                if matches(&item.name) || matches(&item.target_path) {
                    results.push(item);
                }
            }
        }
    }
    results
}

/// 引号包住的目标可包含空格，已经存在的完整路径也可以直接打开。
pub fn split_run_input(input: &str) -> (&str, &str) {
    if let Some(quoted) = input.strip_prefix('"') {
        if let Some((target, args)) = quoted.split_once('"') {
            return (target, args.trim_start());
        }
    }
    if let Some((target, args)) = input.split_once(char::is_whitespace) {
        if !std::path::Path::new(input).exists() {
            return (target, args.trim_start());
        }
    }
    (input, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_search_encodes_the_entire_query_as_one_parameter() {
        let query = "中文 & a+b#0";
        assert_eq!(
            SearchMode::Baidu.web_url(query).as_deref(),
            Some("https://www.baidu.com/s?wd=%E4%B8%AD%E6%96%87%20%26%20a%2Bb%230")
        );
        assert_eq!(
            SearchMode::Google.web_url(query).as_deref(),
            Some("https://www.google.com/search?q=%E4%B8%AD%E6%96%87%20%26%20a%2Bb%230")
        );
    }

    #[test]
    fn run_input_preserves_a_quoted_path_and_its_arguments() {
        assert_eq!(
            split_run_input(r#""C:\Program Files\tool.exe"  "notes file.txt" /open"#),
            (r"C:\Program Files\tool.exe", r#""notes file.txt" /open"#)
        );
    }
}
