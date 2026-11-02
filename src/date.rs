//! 免依赖的日期工具（civil-from-days 算法，Howard Hinnant）。

use std::time::{SystemTime, UNIX_EPOCH};

pub type Ymd = (i32, u32, u32);

pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> Ymd {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y } as i32, m, d)
}

pub fn today() -> Ymd {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    civil_from_days(secs.div_euclid(86400))
}

pub fn today_dayno() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    secs.div_euclid(86400)
}

pub fn dayno(ymd: Ymd) -> i64 {
    days_from_civil(ymd.0, ymd.1, ymd.2)
}

pub fn leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

pub fn valid(ymd: Ymd) -> bool {
    ymd.1 >= 1 && ymd.1 <= 12 && ymd.2 >= 1 && ymd.2 <= days_in_month(ymd.0, ymd.1)
}

pub fn parse(s: &str) -> Option<Ymd> {
    let s = s.trim();
    let mut it = s.split('-');
    let y: i32 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    let ymd = (y, m, d);
    if valid(ymd) {
        Some(ymd)
    } else {
        None
    }
}

pub fn fmt(ymd: Ymd) -> String {
    format!("{:04}-{:02}-{:02}", ymd.0, ymd.1, ymd.2)
}

/// 从文件名里找 YYYY-MM-DD
pub fn from_filename(name: &str) -> Option<Ymd> {
    let b = name.as_bytes();
    if b.len() < 10 {
        return None;
    }
    for i in 0..=b.len() - 10 {
        let w = &b[i..i + 10];
        if w[4] == b'-'
            && w[7] == b'-'
            && w[..4].iter().all(|c| c.is_ascii_digit())
            && w[5..7].iter().all(|c| c.is_ascii_digit())
            && w[8..10].iter().all(|c| c.is_ascii_digit())
        {
            let s = std::str::from_utf8(w).ok()?;
            if let Some(d) = parse(s) {
                return Some(d);
            }
        }
    }
    None
}

pub fn stamp() -> String {
    // 20261102-080431（本地时间粗略换算用 UTC 亦可，仅用于备份目录名）
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for &(y, m, d) in &[(2026, 11, 2), (1970, 1, 1), (2000, 2, 29), (1999, 12, 31)] {
            let z = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(z), (y, m, d));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn parse_and_fmt() {
        assert_eq!(fmt(parse("2026-11-02").unwrap()), "2026-11-02");
        assert!(parse("2026-02-30").is_none());
        assert_eq!(from_filename("2026-10-12-一.md"), Some((2026, 10, 12)));
        assert_eq!(from_filename("并购项目模板.md"), None);
    }
}
