//! 应用状态与按键逻辑。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::config::{self, Config};
use crate::date;
use crate::scan::{self, FileGroup, ScanStats};
use crate::theme::{self, Palette, ThemeName};
use crate::writeback;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Files,
    Tasks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Picker,
    Main,
    Help,
    Confirm,
    Report,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Search,
    Date,
    VaultPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    Save,
    Undo,
    Quit,
}

pub struct PickerEntry {
    pub path: PathBuf,
    pub note: String,
}

pub struct App {
    pub cfg: Config,
    pub vault: Option<PathBuf>,
    pub groups: Vec<FileGroup>,
    pub stats: ScanStats,
    pub date: String,
    pub query: String,
    pub view_files: Vec<usize>,
    pub left: ListState,
    pub right: ListState,
    pub focus: Focus,
    pub screen: Screen,
    pub input_kind: InputKind,
    pub input_buf: String,
    pub pending: Option<Pending>,
    pub confirm_msg: String,
    pub report_title: String,
    pub report: Vec<String>,
    pub toast: Option<(String, Instant)>,
    pub picker: Vec<PickerEntry>,
    pub picker_state: ListState,
    pub theme: ThemeName,
    pub color_ok: bool,
    pub include_templates: bool,
    pub quit: bool,
    pub needs_redraw: bool,
}

fn short(rel: &str) -> String {
    Path::new(rel)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| rel.to_string())
}

fn expand_tilde(s: &str) -> PathBuf {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("~/") {
        config::home().join(rest)
    } else if s == "~" {
        config::home()
    } else {
        PathBuf::from(s)
    }
}

fn find_vaults(dir: &Path, depth: usize, budget: &mut usize, out: &mut Vec<PickerEntry>) {
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for p in dirs {
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        *budget = budget.saturating_sub(1);
        if *budget == 0 {
            return;
        }
        if p.join(".obsidian").is_dir() {
            out.push(PickerEntry { path: p.clone(), note: "含 .obsidian".into() });
        }
        find_vaults(&p, depth - 1, budget, out);
    }
}

fn detect_vaults(cfg: &Config) -> Vec<PickerEntry> {
    let mut out: Vec<PickerEntry> = Vec::new();
    for r in &cfg.recent {
        let p = expand_tilde(r);
        if p.is_dir() {
            out.push(PickerEntry { path: p, note: "最近使用".into() });
        }
    }
    let home = config::home();
    let roots = [
        home.join("Documents"),
        home.join("Nutstore Files"),   // 坚果云同步盘（本机常用，不存在则跳过）
        home.join("iCloud Drive"),
        home.join("Obsidian"),
        home.join("笔记"),
        home.join("AIworks"),
        home.clone(),
    ];
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut budget = 900usize;
        let mut found = Vec::new();
        find_vaults(&root, 3, &mut budget, &mut found);
        for e in found {
            if !out.iter().any(|x| x.path == e.path) {
                out.push(e);
            }
        }
    }
    out
}

impl App {
    pub fn new(cfg: Config, vault: Option<PathBuf>, date_arg: Option<String>, include_templates: bool) -> App {
        let color_ok = theme::color256();
        let th = ThemeName::from_str(cfg.theme.as_str());
        // 没显式给 --vault 就回落到配置里上次用的那个
        let vault = vault.or_else(|| cfg.vault.as_ref().map(PathBuf::from));
        let mut app = App {
            date: date_arg
                .or_else(|| cfg.last_date.clone())
                .filter(|d| date::parse(d).is_some())
                .unwrap_or_else(|| date::fmt(date::today())),
            cfg,
            vault: None,
            groups: Vec::new(),
            stats: ScanStats::default(),
            query: String::new(),
            view_files: Vec::new(),
            left: ListState::default(),
            right: ListState::default(),
            focus: Focus::Tasks,
            screen: Screen::Main,
            input_kind: InputKind::Search,
            input_buf: String::new(),
            pending: None,
            confirm_msg: String::new(),
            report_title: String::new(),
            report: Vec::new(),
            toast: None,
            picker: Vec::new(),
            picker_state: ListState::default(),
            theme: th,
            color_ok,
            include_templates,
            quit: false,
            needs_redraw: true,
        };
        match vault {
            Some(v) if v.is_dir() => {
                app.cfg.remember(&v.to_string_lossy());
                app.vault = Some(v);
                app.rescan();
            }
            Some(v) => {
                app.toast(format!("目录不存在：{}", v.display()));
                app.open_picker();
            }
            None => app.open_picker(),
        }
        app
    }

    // ------------------------------------------------ 基础

    pub fn pal(&self) -> Palette {
        theme::palette(self.theme, self.color_ok)
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
        self.needs_redraw = true;
    }

    pub fn tick(&mut self) {
        if let Some((_, t)) = &self.toast {
            if t.elapsed() > Duration::from_secs(6) {
                self.toast = None;
                self.needs_redraw = true;
            }
        }
    }

    pub fn rescan(&mut self) {
        if let Some(v) = &self.vault {
            let (groups, stats) = scan::scan(v, &self.cfg, self.include_templates);
            self.groups = groups;
            self.stats = stats;
        }
        if self.view_files.is_empty() || true {
            self.left.select(if self.groups.is_empty() { None } else { Some(0) });
        }
        self.right.select(Some(0));
        self.refresh_view();
        self.needs_redraw = true;
    }

    pub fn refresh_view(&mut self) {
        let q = self.query.to_lowercase();
        self.view_files = (0..self.groups.len())
            .filter(|&i| {
                if q.is_empty() {
                    return true;
                }
                let g = &self.groups[i];
                g.rel.to_lowercase().contains(&q)
                    || g.tasks.iter().any(|t| t.body.to_lowercase().contains(&q))
            })
            .collect();
        if self.view_files.is_empty() {
            self.left.select(None);
        } else {
            let cur = self.left.selected().unwrap_or(0).min(self.view_files.len() - 1);
            self.left.select(Some(cur));
        }
        let n = self.visible_tasks().len();
        if n == 0 {
            self.right.select(None);
        } else {
            let cur = self.right.selected().unwrap_or(0).min(n - 1);
            self.right.select(Some(cur));
        }
        self.needs_redraw = true;
    }

    pub fn current_file(&self) -> Option<usize> {
        self.left.selected().and_then(|i| self.view_files.get(i).copied())
    }

    pub fn visible_tasks(&self) -> Vec<usize> {
        let Some(gi) = self.current_file() else { return Vec::new() };
        let q = self.query.to_lowercase();
        self.groups[gi]
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| q.is_empty() || t.body.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn sel_count(&self) -> usize {
        self.groups.iter().flat_map(|g| g.tasks.iter()).filter(|t| t.selected).count()
    }

    pub fn file_sel_count(&self, gi: usize) -> usize {
        self.groups[gi].tasks.iter().filter(|t| t.selected).count()
    }

    pub fn total_tasks(&self) -> usize {
        self.groups.iter().map(|g| g.tasks.len()).sum()
    }

    // ------------------------------------------------ 移动/勾选

    pub fn move_cursor(&mut self, delta: i32) {
        match self.focus {
            Focus::Files => {
                if self.view_files.is_empty() {
                    return;
                }
                let cur = self.left.selected().unwrap_or(0) as i32;
                let next = (cur + delta).clamp(0, self.view_files.len() as i32 - 1) as usize;
                self.left.select(Some(next));
                self.right.select(Some(0));
            }
            Focus::Tasks => {
                let n = self.visible_tasks().len();
                if n == 0 {
                    return;
                }
                let cur = self.right.selected().unwrap_or(0) as i32;
                let next = (cur + delta).clamp(0, n as i32 - 1) as usize;
                self.right.select(Some(next));
            }
        }
        self.needs_redraw = true;
    }

    pub fn jump(&mut self, bottom: bool) {
        match self.focus {
            Focus::Files => {
                if !self.view_files.is_empty() {
                    let i = if bottom { self.view_files.len() - 1 } else { 0 };
                    self.left.select(Some(i));
                    self.right.select(Some(0));
                }
            }
            Focus::Tasks => {
                let n = self.visible_tasks().len();
                if n > 0 {
                    self.right.select(Some(if bottom { n - 1 } else { 0 }));
                }
            }
        }
        self.needs_redraw = true;
    }

    pub fn page(&mut self, dir: i32) {
        self.move_cursor(dir * 10);
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Files => Focus::Tasks,
            Focus::Tasks => Focus::Files,
        };
        self.needs_redraw = true;
    }

    pub fn toggle_current(&mut self) {
        if self.focus == Focus::Files {
            self.toggle_file_all();
            return;
        }
        let Some(gi) = self.current_file() else { return };
        let idxs = self.visible_tasks();
        let Some(&ti) = self.right.selected().and_then(|r| idxs.get(r)) else { return };
        let cur = self.groups[gi].tasks[ti].selected;
        self.groups[gi].tasks[ti].selected = !cur;
        self.needs_redraw = true;
        self.move_cursor(1);
    }

    pub fn toggle_file_all(&mut self) {
        let Some(gi) = self.current_file() else { return };
        let idxs = self.visible_tasks();
        let all = idxs.iter().all(|&ti| self.groups[gi].tasks[ti].selected);
        for &ti in &idxs {
            self.groups[gi].tasks[ti].selected = !all;
        }
        self.toast(if all {
            format!("已取消勾选 {} 项", idxs.len())
        } else {
            format!("已勾选本文件 {} 项", idxs.len())
        });
    }

    pub fn toggle_all_visible(&mut self) {
        let q = self.query.to_lowercase();
        let mut n = 0usize;
        let all = self
            .groups
            .iter()
            .flat_map(|g| g.tasks.iter())
            .filter(|t| q.is_empty() || t.body.to_lowercase().contains(&q))
            .all(|t| t.selected);
        for g in self.groups.iter_mut() {
            for t in g.tasks.iter_mut() {
                if !q.is_empty() && !t.body.to_lowercase().contains(&q) {
                    continue;
                }
                t.selected = !all;
                n += 1;
            }
        }
        self.toast(if all {
            format!("已取消勾选 {n} 项")
        } else {
            format!("已勾选 {n} 项")
        });
    }

    // ------------------------------------------------ 动作

    pub fn ask(&mut self, p: Pending, msg: impl Into<String>) {
        self.pending = Some(p);
        self.confirm_msg = msg.into();
        self.screen = Screen::Confirm;
        self.needs_redraw = true;
    }

    pub fn try_save(&mut self) {
        let n = self.sel_count();
        if n == 0 {
            self.toast("还没有勾选任何条目 —— 用空格勾选");
            return;
        }
        if self.focus == Focus::Files {
            self.toast("先在右侧条目列表里勾选（Tab 切换焦点）");
            return;
        }
        let msg = format!(
            "把 {} 项标记为已完成（completion:: {}）？\n写入前会整份备份，可用 u 撤销。",
            n, self.date
        );
        self.ask(Pending::Save, msg);
    }

    pub fn do_save(&mut self) {
        let Some(vault) = self.vault.clone() else { return };
        let picks: Vec<(String, usize, String)> = self
            .groups
            .iter()
            .flat_map(|g| {
                g.tasks
                    .iter()
                    .filter(|t| t.selected)
                    .map(move |t| (g.rel.clone(), t.line_no, t.body.clone()))
            })
            .collect();
        if picks.is_empty() {
            self.toast("没有可写入的条目");
            return;
        }
        match writeback::apply(&vault, &picks, &self.date) {
            Ok(out) => {
                if out.skipped.is_empty() {
                    let bak = out
                        .backup
                        .as_ref()
                        .map(|b| b.display().to_string())
                        .unwrap_or_default();
                    self.toast(format!(
                        "✅ 已写入 {} 项 / {} 个文件 · 备份 {}",
                        out.changed, out.files, bak
                    ));
                } else {
                    self.report_title = format!("写入 {} 项，跳过 {} 项", out.changed, out.skipped.len());
                    self.report = out
                        .skipped
                        .iter()
                        .map(|s| format!("{} 第 {} 行 — {}", short(&s.rel), s.line, s.reason))
                        .collect();
                    if let Some(b) = &out.backup {
                        self.report.push(format!("备份：{}", b.display()));
                    }
                    self.screen = Screen::Report;
                }
                self.rescan();
                self.screen = if self.screen == Screen::Report { Screen::Report } else { Screen::Main };
            }
            Err(e) => self.toast(format!("写入失败：{e}")),
        }
    }

    pub fn do_undo(&mut self) {
        let Some(vault) = self.vault.clone() else { return };
        match writeback::undo(&vault) {
            Ok(u) => {
                self.toast(format!("↩︎ 已回滚 {} 个文件（备份 {}）", u.restored, u.stamp));
                self.rescan();
            }
            Err(e) => self.toast(format!("回滚失败：{e}")),
        }
    }

    pub fn open_obsidian(&mut self) {
        let Some(gi) = self.current_file() else { return };
        let uri = self.groups[gi].uri.clone();
        match Command::new("xdg-open").arg(&uri).spawn() {
            Ok(_) => self.toast("已在 Obsidian 打开该笔记"),
            Err(e) => self.toast(format!("无法打开 Obsidian：{e}")),
        }
    }

    pub fn open_input(&mut self, kind: InputKind) {
        self.input_kind = kind;
        self.input_buf = match kind {
            InputKind::Search => self.query.clone(),
            InputKind::Date => self.date.clone(),
            InputKind::VaultPath => self
                .vault
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        };
        self.screen = Screen::Input;
        self.needs_redraw = true;
    }

    pub fn open_picker(&mut self) {
        self.picker = detect_vaults(&self.cfg);
        if self.picker.is_empty() {
            self.picker_state.select(None);
        } else {
            self.picker_state.select(Some(0));
        }
        self.screen = Screen::Picker;
        self.needs_redraw = true;
    }

    pub fn pick_vault(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.toast(format!("不是目录：{}", path.display()));
            return;
        }
        self.cfg.remember(&path.to_string_lossy());
        self.vault = Some(path.clone());
        self.screen = Screen::Main;
        self.rescan();
        if !path.join(".obsidian").is_dir() {
            self.toast("提示：该目录没有 .obsidian —— 可能不是 Obsidian vault");
        } else {
            self.toast(format!("已切换到 {}", path.display()));
        }
    }

    pub fn cycle_theme(&mut self) {
        self.theme = self.theme.next();
        self.cfg.theme = self.theme.as_str().to_string();
        self.cfg.save();
        self.toast(format!("配色：{}（{}）", self.theme.label(), self.theme.as_str()));
    }

    // ------------------------------------------------ 按键

    pub fn handle_key(&mut self, k: KeyEvent) -> bool {
        self.needs_redraw = true;
        match self.screen {
            Screen::Help => {
                self.screen = Screen::Main;
            }
            Screen::Confirm => match k.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.screen = Screen::Main;
                    match self.pending.take() {
                        Some(Pending::Save) => self.do_save(),
                        Some(Pending::Undo) => self.do_undo(),
                        Some(Pending::Quit) => self.quit = true,
                        None => {}
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.pending = None;
                    self.screen = Screen::Main;
                }
                _ => {}
            },
            Screen::Report => {
                if matches!(
                    k.code,
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ')
                ) {
                    self.screen = Screen::Main;
                }
            }
            Screen::Input => self.handle_input(k),
            Screen::Picker => self.handle_picker(k),
            Screen::Main => self.handle_main(k),
        }
        !self.quit
    }

    fn handle_input(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => {
                if self.input_kind == InputKind::Search {
                    self.query.clear();
                    self.refresh_view();
                }
                self.screen = if self.vault.is_some() { Screen::Main } else { Screen::Picker };
            }
            KeyCode::Enter => match self.input_kind {
                InputKind::Search => {
                    self.query = self.input_buf.trim().to_string();
                    self.refresh_view();
                    self.screen = Screen::Main;
                }
                InputKind::Date => {
                    let buf = self.input_buf.trim().to_string();
                    if date::parse(&buf).is_none() {
                        self.toast("日期格式应为 YYYY-MM-DD");
                        return;
                    }
                    self.date = buf.clone();
                    self.cfg.last_date = Some(buf);
                    self.cfg.save();
                    self.screen = Screen::Main;
                    self.toast(format!("完成日期：{}", self.date));
                }
                InputKind::VaultPath => {
                    let p = expand_tilde(&self.input_buf);
                    self.screen = Screen::Main;
                    self.pick_vault(p);
                }
            },
            KeyCode::Backspace => {
                self.input_buf.pop();
            }
            KeyCode::Char('u') if ctrl => self.input_buf.clear(),
            KeyCode::Char(c) if !ctrl => {
                self.input_buf.push(c);
            }
            _ => {}
        }
    }

    fn handle_picker(&mut self, k: KeyEvent) {
        let n = self.picker.len();
        match k.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if n > 0 {
                    let cur = self.picker_state.selected().unwrap_or(0);
                    self.picker_state.select(Some((cur + 1).min(n - 1)));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if n > 0 {
                    let cur = self.picker_state.selected().unwrap_or(0);
                    self.picker_state.select(Some(cur.saturating_sub(1)));
                }
            }
            KeyCode::Enter => {
                if let Some(i) = self.picker_state.selected() {
                    if let Some(e) = self.picker.get(i) {
                        let p = e.path.clone();
                        self.pick_vault(p);
                    }
                }
            }
            KeyCode::Char('i') | KeyCode::Char('/') => self.open_input(InputKind::VaultPath),
            KeyCode::Char('x') => {
                if let Some(i) = self.picker_state.selected() {
                    if let Some(e) = self.picker.get(i) {
                        let p = e.path.display().to_string();
                        self.cfg.forget(&p);
                        self.picker.retain(|x| x.path.display().to_string() != p);
                        let n = self.picker.len();
                        if n == 0 {
                            self.picker_state.select(None);
                        } else {
                            self.picker_state.select(Some(i.min(n - 1)));
                        }
                        self.toast("已从最近列表移除");
                    }
                }
            }
            KeyCode::Char('c') => self.cycle_theme(),
            KeyCode::Char('g') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.screen = Screen::Help
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                if self.vault.is_some() {
                    self.screen = Screen::Main;
                } else {
                    self.quit = true;
                }
            }
            _ => {}
        }
    }

    fn handle_main(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Char('q') => {
                if self.sel_count() > 0 {
                    self.ask(Pending::Quit, "还有未保存的勾选，确定退出？");
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Esc => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.refresh_view();
                    self.toast("已清除过滤");
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char('g') if ctrl => self.screen = Screen::Help,
            KeyCode::Char('g') => self.jump(false),
            KeyCode::Char('G') => self.jump(true),
            KeyCode::Char('d') if ctrl => self.page(1),
            KeyCode::Char('u') if ctrl => self.page(-1),
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = Focus::Files;
                self.needs_redraw = true;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.current_file().is_some() {
                    self.focus = Focus::Tasks;
                    self.needs_redraw = true;
                }
            }
            KeyCode::Char(' ') => self.toggle_current(),
            KeyCode::Char('a') => self.toggle_file_all(),
            KeyCode::Char('A') => self.toggle_all_visible(),
            KeyCode::Char('/') => self.open_input(InputKind::Search),
            KeyCode::Char('d') => self.open_input(InputKind::Date),
            KeyCode::Char('u') => {
                let msg = match writeback::last_backup() {
                    Some(b) => format!("把最近一次写入整体回滚？\n备份：{}", b.display()),
                    None => "没有可撤销的备份".to_string(),
                };
                if writeback::last_backup().is_some() {
                    self.ask(Pending::Undo, msg);
                } else {
                    self.toast("没有可撤销的备份");
                }
            }
            KeyCode::Char('r') => {
                let had = self.sel_count();
                self.rescan();
                if had > 0 {
                    self.toast(format!("已重新扫描（{had} 项未保存的勾选已清除）"));
                } else {
                    self.toast("已重新扫描");
                }
            }
            KeyCode::Char('v') => self.open_picker(),
            KeyCode::Char('c') => self.cycle_theme(),
            KeyCode::Char('o') => self.open_obsidian(),
            KeyCode::Char('?') | KeyCode::F(1) => self.screen = Screen::Help,
            KeyCode::Enter => self.try_save(),
            _ => {}
        }
    }
}
