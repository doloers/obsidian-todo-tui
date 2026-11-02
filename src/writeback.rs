//! 写回 vault：逐行校验 → 整份备份 → 落盘；以及撤销。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::state_dir;
use crate::date;
use crate::scan::{mark_done, parse_task};

#[derive(Debug, Clone)]
pub struct Skip {
    pub rel: String,
    pub line: usize,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct Outcome {
    pub changed: usize,
    pub files: usize,
    pub skipped: Vec<Skip>,
    pub backup: Option<PathBuf>,
}

#[derive(Debug, Default, Clone)]
pub struct UndoOutcome {
    pub restored: usize,
    pub stamp: String,
}

pub fn backups_root() -> PathBuf {
    state_dir().join("backups")
}

fn undone_list() -> PathBuf {
    state_dir().join("undone.list")
}

fn read_undone() -> Vec<String> {
    fs::read_to_string(undone_list())
        .map(|t| t.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

fn push_undone(stamp: &str) {
    let _ = fs::create_dir_all(state_dir());
    let mut v = read_undone();
    v.push(stamp.to_string());
    let _ = fs::write(undone_list(), v.join("\n") + "\n");
}

/// 最近一次「尚未撤销」的备份目录
pub fn last_backup() -> Option<PathBuf> {
    let root = backups_root();
    let undone = read_undone();
    let mut dirs: Vec<PathBuf> = fs::read_dir(&root)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("manifest.tsv").is_file())
        .filter(|p| {
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            !undone.contains(&name)
        })
        .collect();
    dirs.sort();
    dirs.pop()
}

fn flat_name(rel: &str) -> String {
    rel.replace('/', "__")
}

/// picks: (相对路径, 行号(1-based), 扫描时的正文)
pub fn apply(vault: &Path, picks: &[(String, usize, String)], date_str: &str) -> io::Result<Outcome> {
    let mut out = Outcome::default();
    let mut by_file: Vec<(String, Vec<&(String, usize, String)>)> = Vec::new();
    for p in picks {
        if let Some(slot) = by_file.iter_mut().find(|(rel, _)| rel == &p.0) {
            slot.1.push(p);
        } else {
            by_file.push((p.0.clone(), vec![p]));
        }
    }

    let stamp = date::stamp();
    let root = backups_root();
    // 同一秒内多次写入：目录名加序号，避免互相覆盖/误删
    let mut bdir = root.join(&stamp);
    let mut seq = 2;
    while bdir.exists() {
        bdir = root.join(format!("{stamp}-{seq}"));
        seq += 1;
    }
    let mut wrote_backup = false;
    let mut manifest = String::new();
    manifest.push_str(&format!("#date\t{date_str}\n"));
    manifest.push_str(&format!("#vault\t{}\n", vault.display()));

    for (rel, sel) in &by_file {
        let abs = vault.join(rel);
        let original = match fs::read_to_string(&abs) {
            Ok(t) => t,
            Err(e) => {
                for p in sel {
                    out.skipped.push(Skip { rel: rel.clone(), line: p.1, reason: format!("读取失败: {e}") });
                }
                continue;
            }
        };
        let mut lines: Vec<String> = original.split('\n').map(|s| s.to_string()).collect();
        let mut changed = 0usize;
        for p in sel {
            let idx = p.1.saturating_sub(1);
            if idx >= lines.len() {
                out.skipped.push(Skip { rel: rel.clone(), line: p.1, reason: "行号越界".into() });
                continue;
            }
            match parse_task(&lines[idx]) {
                None => out.skipped.push(Skip {
                    rel: rel.clone(),
                    line: p.1,
                    reason: "该行已不是任务行".into(),
                }),
                Some((_, true, _)) => out.skipped.push(Skip {
                    rel: rel.clone(),
                    line: p.1,
                    reason: "已经是完成状态".into(),
                }),
                Some((_, false, body)) => {
                    if body.trim() != p.2.trim() {
                        out.skipped.push(Skip {
                            rel: rel.clone(),
                            line: p.1,
                            reason: "内容已变化（同步/编辑过）".into(),
                        });
                        continue;
                    }
                    match mark_done(&lines[idx], date_str) {
                        Some(new) => {
                            lines[idx] = new;
                            changed += 1;
                        }
                        None => out.skipped.push(Skip {
                            rel: rel.clone(),
                            line: p.1,
                            reason: "无法改写该行".into(),
                        }),
                    }
                }
            }
        }
        if changed == 0 {
            continue;
        }
        // 备份 → 落盘
        fs::create_dir_all(&bdir)?;
        wrote_backup = true;
        let flat = flat_name(rel);
        fs::write(bdir.join(&flat), &original)?;
        manifest.push_str(&format!("{rel}\t{flat}\n"));
        fs::write(&abs, lines.join("\n"))?;
        out.changed += changed;
        out.files += 1;
    }

    if out.changed > 0 && wrote_backup {
        fs::write(bdir.join("manifest.tsv"), manifest)?;
        out.backup = Some(bdir);
    } else if wrote_backup {
        let _ = fs::remove_dir_all(&bdir);
    }
    Ok(out)
}

pub fn undo(vault: &Path) -> io::Result<UndoOutcome> {
    let bdir = last_backup()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "没有可撤销的备份"))?;
    let manifest = fs::read_to_string(bdir.join("manifest.tsv"))?;
    let mut restored = 0usize;
    for line in manifest.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let Some((rel, flat)) = line.split_once('\t') else { continue };
        let src = bdir.join(flat);
        if src.is_file() {
            fs::copy(&src, vault.join(rel))?;
            restored += 1;
        }
    }
    let stamp = bdir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    push_undone(&stamp);
    Ok(UndoOutcome { restored, stamp })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> PathBuf {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("ott-{n}"));
        fs::create_dir_all(d.join("vault/03-工作日志")).unwrap();
        fs::create_dir_all(d.join("state")).unwrap();
        std::env::set_var("XDG_STATE_HOME", d.join("state"));
        d
    }

    #[test]
    fn apply_then_undo_roundtrip() {
        let root = scratch();
        let vault = root.join("vault");
        let rel = "03-工作日志/2026-10-12-一.md";
        let file = vault.join(rel);
        let original = "# t\n\n- [ ] 待办A\n> - [ ] 引用B\n- [x] 已完成C\n- [ ] \n";
        fs::write(&file, original).unwrap();

        let picks = vec![
            (rel.to_string(), 3usize, "待办A".to_string()),
            (rel.to_string(), 4usize, "引用B".to_string()),
            (rel.to_string(), 6usize, "".to_string()),
        ];
        let out = apply(&vault, &picks, "2026-11-02").unwrap();
        assert_eq!(out.changed, 3);
        assert_eq!(out.files, 1);
        assert!(out.skipped.is_empty());
        let now = fs::read_to_string(&file).unwrap();
        assert_eq!(
            now,
            "# t\n\n- [x] 待办A [completion:: 2026-11-02]\n> - [x] 引用B [completion:: 2026-11-02]\n- [x] 已完成C\n- [x] [completion:: 2026-11-02]\n"
        );
        assert!(out.backup.as_ref().unwrap().join("manifest.tsv").is_file());

        // 幂等：再点一次同一行，应被跳过而不是写成 [x][x]
        let again = apply(&vault, &picks[0..1], "2026-11-02").unwrap();
        assert_eq!(again.changed, 0);
        assert_eq!(again.skipped.len(), 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), now);

        // 内容漂移：正文对不上就跳过
        let drift = apply(&vault, &[(rel.to_string(), 5usize, "已经改过的文案".to_string())], "2026-11-02").unwrap();
        assert_eq!(drift.changed, 0);
        assert_eq!(drift.skipped.len(), 1);

        // 撤销：逐字节还原，二次撤销报错
        let u = undo(&vault).unwrap();
        assert_eq!(u.restored, 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
        assert!(undo(&vault).is_err());

        fs::remove_dir_all(&root).ok();
    }
}
