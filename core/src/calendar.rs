//! Working-day calendar: weekends, South African public holidays and custom days off.

use std::collections::HashMap;

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};

use crate::model::Settings;

/// One-off public holidays declared under s2A of the Public Holidays Act 36 of 1994.
const SA_SPECIAL: &[(i32, u32, u32, &str)] = &[
    (1999, 6, 2, "Election Day"),
    (1999, 12, 31, "Millennium Holiday"),
    (2000, 1, 2, "Millennium Holiday"),
    (2004, 4, 14, "Election Day"),
    (2006, 3, 1, "Local Election Day"),
    (2008, 5, 2, "Special Holiday"),
    (2009, 4, 22, "Election Day"),
    (2011, 5, 18, "Local Election Day"),
    (2011, 12, 27, "Special Holiday"),
    (2014, 5, 7, "Election Day"),
    (2016, 8, 3, "Local Election Day"),
    (2016, 12, 27, "Special Holiday"),
    (2019, 5, 8, "Election Day"),
    (2021, 11, 1, "Local Election Day"),
    (2022, 12, 27, "Special Holiday"),
    (2023, 12, 15, "Springbok RWC Holiday"),
    (2024, 5, 29, "Election Day"),
    (2026, 11, 4, "Local Election Day"),
];

/// Western (Gregorian) Easter Sunday — anonymous Gregorian algorithm.
pub fn easter_sunday(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).expect("valid easter")
}

/// South African public holidays for a year, including the Sunday → Monday rule.
pub fn sa_holidays(year: i32) -> Vec<(NaiveDate, &'static str)> {
    if year < 1995 {
        return Vec::new();
    }
    let ymd = |m, d| NaiveDate::from_ymd_opt(year, m, d).unwrap();
    let easter = easter_sunday(year);
    let mut base: Vec<(NaiveDate, &'static str)> = vec![
        (ymd(1, 1), "New Year's Day"),
        (ymd(3, 21), "Human Rights Day"),
        (easter - Days::new(2), "Good Friday"),
        (easter + Days::new(1), "Family Day"),
        (ymd(4, 27), "Freedom Day"),
        (ymd(5, 1), "Workers' Day"),
        (ymd(6, 16), "Youth Day"),
        (ymd(8, 9), "National Women's Day"),
        (ymd(9, 24), "Heritage Day"),
        (ymd(12, 16), "Day of Reconciliation"),
        (ymd(12, 25), "Christmas Day"),
        (ymd(12, 26), "Day of Goodwill"),
    ];
    base.extend(
        SA_SPECIAL
            .iter()
            .filter(|s| s.0 == year)
            .map(|&(y, m, d, n)| (NaiveDate::from_ymd_opt(y, m, d).unwrap(), n)),
    );
    let mut out = base.clone();
    for (d, name) in base {
        if d.weekday() == Weekday::Sun {
            let mon = d + Days::new(1);
            if !out.iter().any(|(x, _)| *x == mon) {
                out.push((mon, observed_name(name)));
            }
        }
    }
    out.sort_by_key(|h| h.0);
    out
}

fn observed_name(name: &'static str) -> &'static str {
    match name {
        "New Year's Day" => "New Year's Day (observed)",
        "Human Rights Day" => "Human Rights Day (observed)",
        "Freedom Day" => "Freedom Day (observed)",
        "Workers' Day" => "Workers' Day (observed)",
        "Youth Day" => "Youth Day (observed)",
        "National Women's Day" => "National Women's Day (observed)",
        "Heritage Day" => "Heritage Day (observed)",
        "Day of Reconciliation" => "Day of Reconciliation (observed)",
        "Christmas Day" => "Christmas Day (observed)",
        "Day of Goodwill" => "Day of Goodwill (observed)",
        _ => "Public Holiday (observed)",
    }
}

/// Precomputed lookup of non-working days for a range of years.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkCalendar {
    holidays: HashMap<NaiveDate, String>,
    work_days: [bool; 7],
}

impl WorkCalendar {
    pub fn new(s: &Settings, from_year: i32, to_year: i32) -> Self {
        let mut holidays = HashMap::new();
        if s.sa_holidays {
            for y in from_year..=to_year {
                for (d, n) in sa_holidays(y) {
                    holidays.insert(d, n.to_string());
                }
            }
        }
        for h in &s.custom_holidays {
            holidays.insert(h.date, h.name.clone());
        }
        Self {
            holidays,
            work_days: s.work_days,
        }
    }

    pub fn holiday(&self, d: NaiveDate) -> Option<&str> {
        self.holidays.get(&d).map(String::as_str)
    }

    pub fn is_work_weekday(&self, d: NaiveDate) -> bool {
        self.work_days[d.weekday().num_days_from_monday() as usize]
    }

    pub fn is_working_day(&self, d: NaiveDate) -> bool {
        self.is_work_weekday(d) && !self.holidays.contains_key(&d)
    }

    /// Working days in an inclusive range.
    pub fn working_days(&self, start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
        start
            .iter_days()
            .take_while(|d| *d <= end)
            .filter(|d| self.is_working_day(*d))
            .collect()
    }

    /// Holidays that fall on a working weekday within an inclusive range.
    pub fn skipped_holidays(&self, start: NaiveDate, end: NaiveDate) -> Vec<(NaiveDate, String)> {
        start
            .iter_days()
            .take_while(|d| *d <= end)
            .filter(|d| self.is_work_weekday(*d))
            .filter_map(|d| self.holiday(d).map(|n| (d, n.to_string())))
            .collect()
    }
}

pub fn days_in_month(d: NaiveDate) -> u32 {
    let first = d.with_day(1).unwrap();
    let next = first + Months::new(1);
    (next - first).num_days() as u32
}

pub fn last_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(days_in_month(d)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn easter_dates() {
        assert_eq!(easter_sunday(2024), d(2024, 3, 31));
        assert_eq!(easter_sunday(2025), d(2025, 4, 20));
        assert_eq!(easter_sunday(2026), d(2026, 4, 5));
        assert_eq!(easter_sunday(2027), d(2027, 3, 28));
        assert_eq!(easter_sunday(2019), d(2019, 4, 21));
    }

    #[test]
    fn sunday_rule() {
        // 2026: Freedom Day Mon, Youth Day Tue; 9 Aug 2026 is a Sunday.
        let h = sa_holidays(2026);
        assert!(h.iter().any(|x| x.0 == d(2026, 8, 10)));
        assert!(h.iter().any(|x| x.0 == d(2026, 11, 4)));
        // 2022: Christmas on Sunday, 26th already a holiday, 27th declared specially.
        let h = sa_holidays(2022);
        assert_eq!(h.iter().filter(|x| x.0 == d(2022, 12, 26)).count(), 1);
        assert!(h.iter().any(|x| x.0 == d(2022, 12, 27)));
        // 2000: special 2 Jan was a Sunday -> 3 Jan holiday.
        assert!(sa_holidays(2000).iter().any(|x| x.0 == d(2000, 1, 3)));
    }

    #[test]
    fn working_days_skip_weekends_and_holidays() {
        let cal = WorkCalendar::new(&Settings::default(), 2025, 2025);
        // Mon 22 Sep – Fri 26 Sep 2025, Heritage Day Wed 24th.
        assert_eq!(cal.working_days(d(2025, 9, 22), d(2025, 9, 26)).len(), 4);
        // Easter week 2025: Good Friday 18 Apr, Family Day 21 Apr.
        assert_eq!(cal.working_days(d(2025, 4, 14), d(2025, 4, 25)).len(), 8);
    }
}
