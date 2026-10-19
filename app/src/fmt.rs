//! Number and date formatting.

use chrono::{Datelike, NaiveDate};

/// Days with up to two decimals, trailing zeros trimmed, true minus sign.
pub fn days(v: f64) -> String {
    let v = if v.abs() < 0.005 { 0.0 } else { v };
    let s = format!("{:.2}", v.abs());
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if v < 0.0 {
        format!("−{s}")
    } else {
        s.to_string()
    }
}

pub fn days_signed(v: f64) -> String {
    if v > 0.004 {
        format!("+{}", days(v))
    } else {
        days(v)
    }
}

/// Fixed decimals, used while a number animates so its width doesn't jitter.
pub fn fixed(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    if v < -0.0000001 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        format!("−{s}")
    } else {
        s
    }
}

pub fn decimals_for(v: f64) -> usize {
    let c = (v * 100.0).round() as i64;
    if c % 100 == 0 {
        0
    } else if c % 10 == 0 {
        1
    } else {
        2
    }
}

pub fn plural_days(v: f64) -> String {
    let n = days(v);
    if n == "1" {
        "1 day".into()
    } else {
        format!("{n} days")
    }
}

pub fn date_long(d: NaiveDate) -> String {
    d.format("%a %-d %b %Y").to_string()
}

pub fn date_med(d: NaiveDate) -> String {
    d.format("%-d %b %Y").to_string()
}

pub fn date_short(d: NaiveDate) -> String {
    d.format("%-d %b").to_string()
}

pub fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse_iso(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

pub fn range(a: NaiveDate, b: NaiveDate) -> String {
    if a == b {
        date_long(a)
    } else if a.year() == b.year() && a.month() == b.month() {
        format!("{} – {}", a.format("%a %-d"), date_long(b))
    } else if a.year() == b.year() {
        format!("{} – {}", a.format("%a %-d %b"), date_long(b))
    } else {
        format!("{} – {}", date_long(a), date_long(b))
    }
}

pub fn month_name(m: u32) -> &'static str {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ][(m - 1) as usize]
}

pub fn month_abbr(m: u32) -> &'static str {
    &month_name(m)[..3]
}

/// "today", "tomorrow", "in 3 days", "2 weeks ago" …
pub fn relative(d: NaiveDate, today: NaiveDate) -> String {
    let n = (d - today).num_days();
    let unit = |n: i64| -> String {
        let a = n.abs();
        if a < 14 {
            format!("{a} day{}", if a == 1 { "" } else { "s" })
        } else if a < 60 {
            format!("{} weeks", a / 7)
        } else if a < 730 {
            format!("{} months", a / 30)
        } else {
            format!("{} years", a / 365)
        }
    };
    match n {
        0 => "today".into(),
        1 => "tomorrow".into(),
        -1 => "yesterday".into(),
        n if n > 0 => format!("in {}", unit(n)),
        n => format!("{} ago", unit(n)),
    }
}

pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Human description of how long ago a timestamp (ms) was.
pub fn ago_ms(then: f64, now: f64) -> String {
    let mins = ((now - then) / 60000.0).max(0.0) as i64;
    match mins {
        0 => "just now".into(),
        1..=59 => format!("{mins} min ago"),
        60..=1439 => format!("{} h ago", mins / 60),
        _ => {
            let d = mins / 1440;
            format!("{d} day{} ago", if d == 1 { "" } else { "s" })
        }
    }
}
