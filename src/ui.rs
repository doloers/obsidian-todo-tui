//! 渲染（ratatui + 电致发光黄配色）。

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Focus, InputKind, Screen};
use crate::date;
use crate::theme::Palette;

pub fn draw(f: &mut Frame, app: &mut App) {
    let pal = app.pal();
    f.render_widget(
        Block::default().style(Style::default().bg(pal.bg).fg(pal.fg)),
        f.area(),
    );
    match app.screen {
        Screen::Picker => draw_picker(f, app),
        Screen::Help => {
            let lines = help_lines(app.pal());
            let h = lines.len() as u16 + 2;
            draw_main(f, app);
            draw_modal_box(f, app, " 全部按键 ", lines, 66, h);
        }
        Screen::Confirm => {
            let lines: Vec<Line> = app
                .confirm_msg
                .lines()
                .map(|s| Line::from(vec![Span::styled(s.to_string(), Style::default())]))
                .chain(std::iter::once(Line::from("")))
                .chain(std::iter::once(Line::from(vec![Span::styled(
                    "[y] 确认    [n]/Esc 取消",
                    Style::default().add_modifier(Modifier::BOLD),
                )])))
                .collect();
            let h = lines.len() as u16 + 2;
            draw_main(f, app);
            draw_modal_box(f, app, " 确认 ", lines, 62, h);
        }
        Screen::Report => {
            let title = format!(" {} ", app.report_title);
            let lines: Vec<Line> = app
                .report
                .iter()
                .map(|s| Line::from(vec![Span::styled(s.clone(), Style::default())]))
                .chain(std::iter::once(Line::from("")))
                .chain(std::iter::once(Line::from(vec![Span::styled(
                    "Enter/Esc 关闭",
                    Style::default().add_modifier(Modifier::DIM),
                )])))
                .collect();
            draw_main(f, app);
            let h = (lines.len() as u16 + 4).min(70);
            draw_modal_box(f, app, &title, lines, 70, h);
        }
        _ => draw_main(f, app),
    }
}

// ---------------------------------------------------------------- 文本工具

fn fit(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

fn tail(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<char> = Vec::new();
    let mut w = 0usize;
    for ch in chars.iter().rev() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > width.saturating_sub(1) {
            break;
        }
        out.push(*ch);
        w += cw;
    }
    out.reverse();
    format!("…{}", out.into_iter().collect::<String>())
}

fn short_path(p: &std::path::Path) -> String {
    let home = crate::config::home();
    let s = p.display().to_string();
    let h = home.display().to_string();
    s.strip_prefix(&h).map(|r| format!("~{r}")).unwrap_or(s)
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    let width = width.max(4);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > width {
            out.push(std::mem::take(&mut cur));
            w = 0;
        }
        cur.push(ch);
        w += cw;
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

// ---------------------------------------------------------------- 主界面

fn block(pal: Palette, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { pal.hi } else { pal.line }))
        .style(Style::default().bg(pal.bg).fg(pal.fg))
}

fn draw_main(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(6),
        Constraint::Length(4),
    ])
    .split(area);
    draw_header(f, app, rows[0]);
    let cols =
        Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)]).split(rows[1]);
    draw_files(f, app, cols[0]);
    draw_tasks(f, app, cols[1]);
    draw_footer(f, app, rows[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.pal();
    let vault = app
        .vault
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "（未选择 vault）".into());
    let inner = area.width.saturating_sub(4) as usize;
    let line1 = Line::from(vec![
        Span::styled(
            "▌",
            Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            fit(&vault, inner.saturating_sub(24)),
            Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  ·  未完成 {} 项 / {} 文件",
                app.total_tasks(),
                app.groups.len()
            ),
            Style::default().fg(pal.dim),
        ),
    ]);
    let mut l2 = vec![
        Span::styled("已勾选 ", Style::default().fg(pal.dim)),
        Span::styled(
            format!("{}", app.sel_count()),
            Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" 项   ", Style::default().fg(pal.dim)),
        Span::styled("完成日期 ", Style::default().fg(pal.dim)),
        Span::styled(app.date.clone(), Style::default().fg(pal.fg)),
        Span::styled(
            format!("   配色 {}", app.theme.label()),
            Style::default().fg(pal.dim),
        ),
    ];
    if !app.query.is_empty() {
        l2.push(Span::styled(
            format!("   过滤 “{}”", app.query),
            Style::default().fg(pal.hi),
        ));
    }
    if app.stats.skipped_template > 0 {
        l2.push(Span::styled(
            format!("   已隐藏 {} 个模板文件", app.stats.skipped_template),
            Style::default().fg(pal.dim),
        ));
    }
    let p = Paragraph::new(Text::from(vec![line1, Line::from(l2)]))
        .block(block(pal, false).title(Span::styled(
            " Obsidian 待办核对 ",
            Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
        )));
    f.render_widget(p, area);
}

fn draw_files(f: &mut Frame, app: &mut App, area: Rect) {
    let pal = app.pal();
    let inner_w = area.width.saturating_sub(6) as usize;
    let mut items: Vec<ListItem> = Vec::new();
    for &gi in &app.view_files {
        let g = &app.groups[gi];
        let date_s = match g.date {
            Some(d) => date::fmt(d),
            None => "无日期文件".to_string(),
        };
        let age_s = match g.age {
            Some(a) => format!("{a}天"),
            None => String::new(),
        };
        let sel_n = app.file_sel_count(gi);
        let mut spans = vec![
            Span::styled(date_s.clone(), Style::default().fg(pal.fg)),
            Span::styled(
                format!(" {:>2}项 ", g.tasks.len()),
                Style::default().fg(pal.hi),
            ),
            Span::styled(format!("{age_s:>5}"), Style::default().fg(pal.dim)),
        ];
        if sel_n > 0 {
            spans.push(Span::styled(
                format!(" ✎{sel_n}"),
                Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
            ));
        }
        let used = date_s.width() + 5 + 5 + if sel_n > 0 { 4 } else { 0 };
        let rest = inner_w.saturating_sub(used + 1);
        if rest >= 10 {
            spans.push(Span::styled(
                format!(" {}", tail(&g.rel, rest)),
                Style::default().fg(pal.dim),
            ));
        }
        items.push(ListItem::new(Text::from(vec![Line::from(spans)])));
    }
    if items.is_empty() {
        items.push(ListItem::new(Text::from(vec![Line::from(vec![
            Span::styled(
                if app.query.is_empty() {
                    "没有未完成的待办 🎉"
                } else {
                    "没有匹配的条目"
                }
                .to_string(),
                Style::default().fg(pal.dim),
            ),
        ])])));
    }
    let pos = app
        .left
        .selected()
        .map(|i| format!(" {}/{} ", i + 1, app.view_files.len().max(1)))
        .unwrap_or_default();
    let title = Line::from(vec![Span::styled(
        " 文件 ",
        Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
    )]);
    let mut b = block(pal, app.focus == Focus::Files).title(title);
    if !pos.is_empty() {
        b = b.title_top(Line::from(Span::styled(pos, Style::default().fg(pal.dim))).alignment(Alignment::Right));
    }
    let list = List::new(items)
        .block(b)
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .bg(pal.sel_bg)
                .fg(pal.sel_fg)
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(list, area, &mut app.left);
}

fn draw_tasks(f: &mut Frame, app: &mut App, area: Rect) {
    let pal = app.pal();
    let body_w = area.width.saturating_sub(2 + 2 + 4) as usize;
    let idxs = app.visible_tasks();
    let mut items: Vec<ListItem> = Vec::new();
    if let Some(gi) = app.current_file() {
        for &ti in &idxs {
            let t = &app.groups[gi].tasks[ti];
            let body = if t.body.trim().is_empty() {
                "（空任务行）".to_string()
            } else {
                t.body.clone()
            };
            let (prefix, prefix_style, body_style) = if t.selected {
                (
                    "[x] ",
                    Style::default().fg(pal.sel_bg).add_modifier(Modifier::BOLD),
                    Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    "[ ] ",
                    Style::default().fg(pal.dim),
                    Style::default().fg(pal.fg),
                )
            };
            let segs = wrap(&body, body_w);
            let mut lines: Vec<Line> = Vec::new();
            for (i, seg) in segs.into_iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![
                        Span::styled(prefix.to_string(), prefix_style),
                        Span::styled(seg, body_style),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(seg, body_style),
                    ]));
                }
            }
            items.push(ListItem::new(Text::from(lines)));
        }
    }
    if items.is_empty() {
        items.push(ListItem::new(Text::from(vec![Line::from(vec![
            Span::styled(
                if app.vault.is_none() {
                    "按 v 选择 vault"
                } else if app.query.is_empty() {
                    "这个文件没有条目"
                } else {
                    "没有匹配的条目"
                }
                .to_string(),
                Style::default().fg(pal.dim),
            ),
        ])])));
    }

    let (fname, cur_line, pos) = match app.current_file() {
        Some(gi) => {
            let g = &app.groups[gi];
            let fname = std::path::Path::new(&g.rel)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| g.rel.clone());
            let line = app
                .right
                .selected()
                .and_then(|r| idxs.get(r))
                .map(|&ti| format!(" · 第{}行", g.tasks[ti].line_no))
                .unwrap_or_default();
            let pos = app
                .right
                .selected()
                .map(|r| format!("{}/{}", r + 1, idxs.len().max(1)))
                .unwrap_or_default();
            (fname, line, pos)
        }
        None => (String::new(), String::new(), String::new()),
    };
    let title = Line::from(vec![Span::styled(
        " 条目 ",
        Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
    )]);
    let mut b = block(pal, app.focus == Focus::Tasks).title(title);
    let avail = (area.width.saturating_sub(2) as usize).saturating_sub(6);
    let fname_short = fit(&fname, avail.saturating_sub(cur_line.width() + pos.width() + 3));
    let right = format!("{fname_short}{cur_line} · {pos} ");
    if !right.trim().is_empty() {
        b = b.title_top(
            Line::from(Span::styled(right, Style::default().fg(pal.dim))).alignment(Alignment::Right),
        );
    }
    let list = List::new(items)
        .block(b)
        .highlight_symbol("› ")
        .highlight_style(Style::default().bg(pal.sel_bg).fg(pal.sel_fg));
    f.render_stateful_widget(list, area, &mut app.right);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.pal();
    let inner_w = area.width.saturating_sub(4) as usize;
    let k1 = if app.screen == Screen::Input {
        match app.input_kind {
            InputKind::Search => "输入过滤词（企业 / 项目名）· Enter 确认 · Esc 取消",
            InputKind::Date => "输入完成日期 YYYY-MM-DD · Enter 确认 · Esc 取消",
            InputKind::VaultPath => "输入 vault 路径（支持 ~）· Enter 确认 · Esc 取消",
        }
        .to_string()
    } else {
        let left = "␣ 勾选   │   ⏎ 写入   │   u 撤销   │   / 过滤";
        let right = "^G 全部按键";
        let gap = inner_w.saturating_sub(UnicodeWidthStr::width(left) + UnicodeWidthStr::width(right));
        if gap >= 2 {
            format!("{left}{}{right}", " ".repeat(gap))
        } else {
            fit(&format!("{left}   {right}"), inner_w)
        }
    };
    let status = if app.screen == Screen::Input {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(pal.hi).add_modifier(Modifier::BOLD)),
            Span::styled(app.input_buf.clone(), Style::default().fg(pal.fg)),
            Span::styled("▏", Style::default().fg(pal.hi)),
        ])
    } else if let Some((msg, _)) = &app.toast {
        Line::from(Span::styled(
            fit(msg, inner_w),
            Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(vec![
            Span::styled("备份 ", Style::default().fg(pal.dim)),
            Span::styled(
                short_path(&crate::writeback::backups_root()),
                Style::default().fg(pal.dim),
            ),
            Span::styled(
                "   ·   写前整份备份，可连续撤销",
                Style::default().fg(pal.dim),
            ),
        ])
    };
    let lines = vec![
        Line::from(Span::styled(fit(&k1, inner_w), Style::default().fg(pal.fg))),
        status,
    ];
    f.render_widget(
        Paragraph::new(Text::from(lines)).block(block(pal, false)),
        area,
    );
}

// ---------------------------------------------------------------- 弹窗

fn draw_modal_box(f: &mut Frame, app: &App, title: &str, lines: Vec<Line>, pct_w: u16, h: u16) {
    let pal = app.pal();
    let area = f.area();
    let w = (area.width * pct_w / 100).clamp(30, area.width);
    let h = h.clamp(6, area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let p = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.hi))
                .style(Style::default().bg(pal.bg).fg(pal.fg))
                .title(Span::styled(
                    title.to_string(),
                    Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(p, rect);
}

fn help_lines(pal: Palette) -> Vec<Line<'static>> {
    let rows: &[(&str, &str)] = &[
        ("移动", "j / k ↑↓　上下      g / G　首尾      ^D / ^U　翻页"),
        ("切栏", "h / l  或  Tab"),
        ("勾选", "空格　当前项      a　本文件      A　全部"),
        ("筛选", "/　过滤      Esc　清除过滤"),
        ("写入", "⏎　写入      d　完成日期      u　撤销"),
        ("库与外观", "r　重扫      v　换库      c　配色      o　打开"),
        ("其它", "?  或  ^G　本帮助      q　退出"),
    ];
    let mut v: Vec<Line<'static>> = Vec::new();
    for (k, d) in rows {
        v.push(Line::from(vec![
            Span::styled(
                format!("{k:<12}"),
                Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
            ),
            Span::styled((*d).to_string(), Style::default().fg(pal.fg)),
        ]));
    }
    v.push(Line::from(""));
    v.push(Line::from(Span::styled(
        "- [ ] X   →   - [x] X [completion:: YYYY-MM-DD]",
        Style::default().fg(pal.dim),
    )));
    v.push(Line::from(Span::styled(
        "写入前逐行校验；整份备份，可连续撤销",
        Style::default().fg(pal.dim),
    )));
    v.push(Line::from(Span::styled(
        format!("备份 {}", short_path(&crate::writeback::backups_root())),
        Style::default().fg(pal.dim),
    )));
    v.push(Line::from(Span::styled(
        "按任意键关闭",
        Style::default().fg(pal.dim).add_modifier(Modifier::DIM),
    )));
    v
}

// ---------------------------------------------------------------- vault 选择

fn draw_picker(f: &mut Frame, app: &mut App) {
    let pal = app.pal();
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(4),
    ])
    .split(f.area());

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "选择 Obsidian vault ",
                Style::default().fg(pal.hi).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "j/k 移动 · Enter 进入 · i 输入路径 · x 移除记录",
                Style::default().fg(pal.dim),
            ),
        ]))
        .block(block(pal, false)),
        rows[0],
    );

    let mut items: Vec<ListItem> = Vec::new();
    for e in &app.picker {
        items.push(ListItem::new(Text::from(vec![Line::from(vec![
            Span::styled(e.path.display().to_string(), Style::default().fg(pal.fg)),
            Span::styled(format!("  {}", e.note), Style::default().fg(pal.dim)),
        ])])));
    }
    if items.is_empty() {
        items.push(ListItem::new(Text::from(vec![Line::from(vec![
            Span::styled(
                "没自动找到 vault —— 按 i 输入路径".to_string(),
                Style::default().fg(pal.dim),
            ),
        ])])));
    }
    let list = List::new(items)
        .block(block(pal, true).title(Span::styled(
            " 检测到的 vault ",
            Style::default().fg(pal.hi),
        )))
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .bg(pal.sel_bg)
                .fg(pal.sel_fg)
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(list, rows[1], &mut app.picker_state);

    let body = vec![Line::from(Span::styled(
        "配置 ~/.config/obsidian-todo-tui/config.toml（下次启动直接进上次的 vault）",
        Style::default().fg(pal.dim),
    ))];
    f.render_widget(
        Paragraph::new(Text::from(body)).block(block(pal, false)),
        rows[2],
    );
}
