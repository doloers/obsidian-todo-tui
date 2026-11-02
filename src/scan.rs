//! 扫描 vault：收集所有未勾选的任务行（`- [ ]`，含 `> - [ ]` 引用式）。

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::date::{self, Ymd};

#[derive(Debug, Clone)]
pub struct Task {
    pub line_no: usize, // 1-based
    pub body: String,   // 已 trim
    pub selected: bool, // 界面上勾选（待写回）
}

#[derive(Debug, Clone)]
pub struct FileGroup {
    pub rel: String, // 相对路径，'/' 分隔
    pub date: Option<Ymd>,
    pub age: Option<i64>,
    pub tasks: Vec<Task>,
    pub uri: String,
}

#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub files_scanned: usize,
    pub files_with_tasks: usize,
    pub skipped_template: usize,
    pub errors: Vec<String>,
}

/// 解析一行任务。返回 (mark 在行内的字节下标, 是否已完成, 正文)。
pub fn parse_task(line: &str) -> Option<(usize, bool, &str)> {
    let b = line.as_bytes();
    let mut i = 0usize;
    // 跳过行首空白与引用前缀 `>`（可多级）
    loop {
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        if i < b.len() && b[i] == b'>' {
            i += 1;
            continue;
        }
        break;
    }
    if i >= b.len() || !matches!(b[i], b'-' | b'*' | b'+') {
        return None;
    }
    i += 1;
    let ws = i;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    if i == ws {
        return None;
    }
    if i + 3 > b.len() {
        return None; // 至少要有 "[?]"
    }
    if b[i] != b'[' {
        return None;
    }
    let mark = i + 1;
    if !matches!(b[mark], b' ' | b'x' | b'X') {
        return None;
    }
    if b[i + 2] != b']' {
        return None;
    }
    let checked = b[mark] != b' ';
    Some((mark, checked, line[i + 3..].trim()))
}

/// 把一行未完成任务改成已完成（并补 completion 元数据）。内容不符则 None。
pub fn mark_done(line: &str, date: &str) -> Option<String> {
    let (mark, checked, _) = parse_task(line)?;
    if checked {
        return None;
    }
    // 保留 CRLF 行尾
    let (core, eol) = if let Some(stripped) = line.strip_suffix('\r') {
        (stripped, "\r")
    } else {
        (line, "")
    };
    let mut out = String::with_capacity(core.len() + 40);
    out.push_str(&core[..mark]);
    out.push('x');
    out.push_str(&core[mark + 1..]);
    if !out.contains("completion::") {
        let trimmed = out.trim_end().to_string();
        out = format!("{trimmed} [completion:: {date}]");
    }
    out.push_str(eol);
    Some(out)
}

fn url_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for &b in s.as_bytes() {
        let ok = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'_' | b'.' | b'~')
            || (keep_slash && b == b'/');
        if ok {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn excluded(rel: &str, cfg: &Config, include_templates: bool) -> bool {
    for d in &cfg.exclude_dirs {
        if rel == d || rel.starts_with(&format!("{d}/")) || rel.contains(&format!("/{d}/")) {
            return true;
        }
    }
    if !include_templates {
        for s in &cfg.exclude_substrings {
            if !s.is_empty() && rel.contains(s) {
                return true;
            }
        }
    }
    false
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if p.is_dir() {
            if name.starts_with('.') {
                continue;
            }
            walk(&p, out);
        } else if name.to_lowercase().ends_with(".md") {
            out.push(p);
        }
    }
}

pub fn scan(vault: &Path, cfg: &Config, include_templates: bool) -> (Vec<FileGroup>, ScanStats) {
    let mut files = Vec::new();
    walk(vault, &mut files);
    let today = date::today_dayno();
    let vault_name = vault
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "vault".into());
    let mut groups = Vec::new();
    let mut stats = ScanStats::default();

    for abs in files {
        let rel = abs
            .strip_prefix(vault)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| abs.to_string_lossy().to_string());
        if excluded(&rel, cfg, include_templates) {
            if cfg.exclude_substrings.iter().any(|s| !s.is_empty() && rel.contains(s)) {
                stats.skipped_template += 1;
            }
            continue;
        }
        stats.files_scanned += 1;
        let text = match fs::read_to_string(&abs) {
            Ok(t) => t,
            Err(e) => {
                stats.errors.push(format!("{rel}: {e}"));
                continue;
            }
        };
        let mut tasks = Vec::new();
        for (idx, line) in text.split('\n').enumerate() {
            if let Some((_, false, body)) = parse_task(line) {
                tasks.push(Task {
                    line_no: idx + 1,
                    body: body.to_string(),
                    selected: false,
                });
            }
        }
        if tasks.is_empty() {
            continue;
        }
        stats.files_with_tasks += 1;
        let fname = abs.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let d = date::from_filename(&fname);
        let uri = format!(
            "obsidian://open?vault={}&file={}",
            url_encode(&vault_name, false),
            url_encode(rel.trim_end_matches(".md"), true)
        );
        groups.push(FileGroup {
            rel,
            date: d,
            age: d.map(|x| today - date::dayno(x)),
            tasks,
            uri,
        });
    }

    groups.sort_by(|a, b| match (a.date, b.date) {
        (Some(x), Some(y)) => date::dayno(y).cmp(&date::dayno(x)).then_with(|| a.rel.cmp(&b.rel)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.rel.cmp(&b.rel),
    });
    (groups, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variants() {
        assert_eq!(parse_task("- [ ] 待办"), Some((3, false, "待办")));
        assert_eq!(parse_task("> - [ ] 引用"), Some((5, false, "引用")));
        assert_eq!(parse_task("  * [x] 完成"), Some((5, true, "完成")));
        assert_eq!(parse_task("- [x]"), Some((3, true, ""))); // 空正文也算任务行
        assert!(parse_task("- [[链接]] 不是任务").is_none());
        assert!(parse_task("普通文字").is_none());
        assert!(parse_task("- []").is_none());
    }

    #[test]
    fn marks_done() {
        assert_eq!(
            mark_done("- [ ] 待办A", "2026-11-02").unwrap(),
            "- [x] 待办A [completion:: 2026-11-02]"
        );
        assert_eq!(
            mark_done("> - [ ] 引用", "2026-11-02").unwrap(),
            "> - [x] 引用 [completion:: 2026-11-02]"
        );
        // 已有 completion 时不重复追加
        assert_eq!(
            mark_done("- [ ] x [completion:: 2020-01-01]", "2026-11-02").unwrap(),
            "- [x] x [completion:: 2020-01-01]"
        );
        // CRLF 保留
        assert_eq!(
            mark_done("- [ ] a\r", "2026-11-02").unwrap(),
            "- [x] a [completion:: 2026-11-02]\r"
        );
        assert!(mark_done("- [x] 已完成", "2026-11-02").is_none());
    }

    #[test]
    fn encodes_uri() {
        assert_eq!(url_encode("我的笔记", false), "%E6%88%91%E7%9A%84%E7%AC%94%E8%AE%B0");
        assert_eq!(url_encode("03-工作日志/2026/a", true), "03-%E5%B7%A5%E4%BD%9C%E6%97%A5%E5%BF%97/2026/a");
    }
}
