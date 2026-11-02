//! 配色：与 power-tui 同一套「电致发光黄」（256 色索引一致，互相对得上）。
//!
//! amber —— 电致发光屏：暗底 + 琥珀黄字（默认）
//! paper —— 电纸黄：黄纸底 + 暖墨字
//!
//! 只用 256 色立方里的近似值，不动终端调色板；不足 256 色时退化成 8 色黄/黑。

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Amber,
    Paper,
}

impl ThemeName {
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeName::Amber => "amber",
            ThemeName::Paper => "paper",
        }
    }
    pub fn from_str(s: &str) -> ThemeName {
        match s.trim() {
            "paper" => ThemeName::Paper,
            _ => ThemeName::Amber,
        }
    }
    pub fn next(self) -> ThemeName {
        match self {
            ThemeName::Amber => ThemeName::Paper,
            ThemeName::Paper => ThemeName::Amber,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ThemeName::Amber => "琥珀",
            ThemeName::Paper => "电纸",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub hi: Color,
    pub dim: Color,
    pub sel_bg: Color,
    pub sel_fg: Color,
    pub line: Color,
}

/// 终端是否支持 256 色（COLORTERM 或 TERM 里带 256）。
pub fn color256() -> bool {
    if let Ok(ct) = std::env::var("COLORTERM") {
        let ct = ct.to_lowercase();
        if ct.contains("truecolor") || ct.contains("24bit") || ct.contains("256") {
            return true;
        }
    }
    std::env::var("TERM")
        .map(|t| t.contains("256"))
        .unwrap_or(false)
}

pub fn palette(theme: ThemeName, color_ok: bool) -> Palette {
    if !color_ok {
        // 降级：8 色黄/黑（与 power-tui 一致）
        return match theme {
            _ => Palette {
                bg: Color::Black,
                fg: Color::Yellow,
                hi: Color::LightYellow,
                dim: Color::Yellow,
                sel_bg: Color::Yellow,
                sel_fg: Color::Black,
                line: Color::Yellow,
            },
        };
    }
    match theme {
        ThemeName::Amber => Palette {
            bg: Color::Indexed(234),
            fg: Color::Indexed(222),
            hi: Color::Indexed(220),
            dim: Color::Indexed(137),
            sel_bg: Color::Indexed(214),
            sel_fg: Color::Indexed(234),
            line: Color::Indexed(94),
        },
        // paper：power-tui 里 hi=232（浅色，作反白底用）；本程序 hi 是强调文字色，
        // 因此换成琥珀棕 136，保证黄纸底上可读。
        ThemeName::Paper => Palette {
            bg: Color::Indexed(230),
            fg: Color::Indexed(235),
            hi: Color::Indexed(136),
            dim: Color::Indexed(138),
            sel_bg: Color::Indexed(214),
            sel_fg: Color::Indexed(235),
            line: Color::Indexed(137),
        },
    }
}
