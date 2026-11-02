//! 配置：记住 vault 路径 / 最近使用 / 排除规则。纯手写 key=value 文本，零依赖。

use std::fs;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

pub fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".config"));
    base.join("obsidian-todo-tui").join("config.toml")
}

pub fn state_dir() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".local/state"));
    base.join("obsidian-todo-tui")
}

#[derive(Debug, Clone)]
pub struct Config {
    pub vault: Option<String>,
    pub recent: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub exclude_substrings: Vec<String>,
    pub last_date: Option<String>,
    pub theme: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            vault: None,
            recent: Vec::new(),
            exclude_dirs: vec![".obsidian".into(), ".trash".into(), "06-帮助".into()],
            exclude_substrings: vec!["模板".into()],
            last_date: None,
            theme: "amber".into(),
        }
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(s);
    s.replace("\\\"", "\"").replace("\\\\", "\\")
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn parse_list(v: &str) -> Vec<String> {
    let v = v.trim();
    let inner = v
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .unwrap_or(v);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut esc = false;
    for ch in inner.chars() {
        if in_str {
            if esc {
                cur.push(ch);
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
                out.push(std::mem::take(&mut cur));
            } else {
                cur.push(ch);
            }
        } else if ch == '"' {
            in_str = true;
        }
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn write_list(out: &mut String, key: &str, list: &[String]) {
    out.push_str(key);
    out.push_str(" = [");
    for (i, s) in list.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&quote(s));
    }
    out.push_str("]\n");
}

impl Config {
    pub fn load() -> Config {
        let mut cfg = Config::default();
        let path = config_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return cfg;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim();
            match k {
                "vault" => {
                    let val = unquote(v);
                    cfg.vault = if val.is_empty() { None } else { Some(val) };
                }
                "recent" => cfg.recent = parse_list(v),
                "exclude_dirs" => cfg.exclude_dirs = parse_list(v),
                "exclude_substrings" => cfg.exclude_substrings = parse_list(v),
                "theme" => cfg.theme = unquote(v),
                "last_date" => {
                    let val = unquote(v);
                    cfg.last_date = if val.is_empty() { None } else { Some(val) };
                }
                _ => {}
            }
        }
        cfg
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut out = String::from("# obsidian-todo-tui 配置（可手改）\n");
        out.push_str(&format!("vault = {}\n", quote(self.vault.as_deref().unwrap_or(""))));
        out.push_str(&format!(
            "last_date = {}\n",
            quote(self.last_date.as_deref().unwrap_or(""))
        ));
        out.push_str(&format!("theme = {}\n", quote(&self.theme)));
        write_list(&mut out, "recent", &self.recent);
        write_list(&mut out, "exclude_dirs", &self.exclude_dirs);
        write_list(&mut out, "exclude_substrings", &self.exclude_substrings);
        let _ = fs::write(&path, out);
    }

    pub fn remember(&mut self, path: &str) {
        self.vault = Some(path.to_string());
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(8);
        self.save();
    }

    pub fn forget(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        if self.vault.as_deref() == Some(path) {
            self.vault = None;
        }
        self.save();
    }
}
