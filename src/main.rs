//! obsidian-todo-tui —— Obsidian 待办核对 TUI
//!
//! 勾选已完成的历史待办，直接写回 vault（写前整份备份，可撤销）。

mod app;
mod config;
mod date;
mod scan;
mod theme;
mod ui;
mod writeback;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

struct Args {
    vault: Option<PathBuf>,
    date: Option<String>,
    list: bool,
    all: bool,
    help: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { vault: None, date: None, list: false, all: false, help: false };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let take_next = |name: &str| -> Result<String, String> {
            if i + 1 < argv.len() {
                Ok(argv[i + 1].clone())
            } else {
                Err(format!("{name} 缺少参数"))
            }
        };
        match arg {
            "-h" | "--help" => a.help = true,
            "--list" | "-l" => a.list = true,
            "--all" => a.all = true,
            "-v" | "--vault" => {
                a.vault = Some(PathBuf::from(take_next("--vault")?));
                i += 1;
            }
            "-d" | "--date" => {
                a.date = Some(take_next("--date")?);
                i += 1;
            }
            _ if arg.starts_with("--vault=") => {
                a.vault = Some(PathBuf::from(arg.trim_start_matches("--vault=")))
            }
            _ if arg.starts_with("--date=") => {
                a.date = Some(arg.trim_start_matches("--date=").to_string())
            }
            other => return Err(format!("未知参数：{other}（--help 看用法）")),
        }
        i += 1;
    }
    Ok(a)
}

const HELP: &str = "\
obsidian-todo-tui —— Obsidian 待办核对 TUI

用法:
  obsidian-todo-tui [--vault DIR] [--date YYYY-MM-DD] [--all]
  obsidian-todo-tui --list [--vault DIR]     只在终端列出未完成任务
  obsidian-todo-tui --help

选项:
  -v, --vault DIR       vault 目录（不给则用上次的，或进选择界面）
  -d, --date  YYYY-MM-DD 写入时用的 completion 日期（默认今天）
      --all             连文件名含「模板」的笔记一起扫
  -l, --list            打印清单后退出（脚本可用）
  -h, --help            显示本帮助

配置文件: ~/.config/obsidian-todo-tui/config.toml
备份目录: ~/.local/state/obsidian-todo-tui/backups/
";

fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, cursor::Show);
}

fn run(mut app: app::App) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Clear(ClearType::All), cursor::Hide)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        prev(info);
    }));

    let result = loop {
        if app.needs_redraw {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            app.needs_redraw = false;
        }
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    if !app.handle_key(k) {
                        break Ok(());
                    }
                }
                Event::Resize(_, _) => app.needs_redraw = true,
                _ => {}
            }
        }
        app.tick();
    };

    let _ = std::panic::take_hook();
    restore();
    result
}

fn print_list(vault: Option<PathBuf>, cfg: &config::Config, include_templates: bool) -> io::Result<()> {
    let Some(vault) = vault else {
        eprintln!("没有指定 vault，也没有历史配置：请用 --vault DIR 指定");
        return Ok(());
    };
    if !vault.is_dir() {
        eprintln!("vault 不存在：{}", vault.display());
        return Ok(());
    }
    let (groups, stats) = scan::scan(&vault, cfg, include_templates);
    println!("vault: {}", vault.display());
    if stats.skipped_template > 0 {
        println!("（已跳过 {} 个模板文件，--all 可包含）", stats.skipped_template);
    }
    let mut total = 0;
    for g in &groups {
        println!("\n## {}  {}", g.date.map(date::fmt).unwrap_or_else(|| "—".into()), g.rel);
        for t in &g.tasks {
            total += 1;
            let age = g.age.map(|a| format!("{a}d")).unwrap_or_else(|| "-".into());
            println!(
                "  L{:<4} [{:>5}] {}",
                t.line_no,
                age,
                if t.body.trim().is_empty() { "（空任务行）" } else { t.body.as_str() }
            );
        }
    }
    println!("\n合计 {} 项 / {} 个文件", total, groups.len());
    Ok(())
}

fn main() -> io::Result<()> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if args.help {
        print!("{HELP}");
        return Ok(());
    }
    let cfg = config::Config::load();
    if args.list {
        let vault = args
            .vault
            .clone()
            .or_else(|| cfg.vault.as_ref().map(PathBuf::from));
        return print_list(vault, &cfg, args.all);
    }
    let app = app::App::new(cfg, args.vault, args.date, args.all);
    run(app)
}
