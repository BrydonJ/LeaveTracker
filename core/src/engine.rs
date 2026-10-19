//! The ledger engine. It replays every day from the start date, applying
//! accrual, adjustments, leave taken, expiry and cycle close in a fixed order,
//! so any balance on any day can be traced back to individual movements.
//!
//! Order of operations within a day:
//! 1. carried-over days whose use-by date has passed expire;
//! 2. the day's accrual is earned (credited daily, or at month end);
//! 3. adjustments are applied;
//! 4. annual leave taken that day is deducted;
//! 5. on the last day of a leave cycle the balance is capped and carried over.

use std::collections::HashMap;

use chrono::{Datelike, Days, Months, NaiveDate};

use crate::calendar::{days_in_month, WorkCalendar};
use crate::model::{
    AdjustmentKind, CreditTiming, CycleBasis, Data, LeaveEntry, RateBasis, Settings,
};

const EPS: f64 = 1e-9;

/// Number of completed years of service on `d`.
pub fn years_of_service(start: NaiveDate, d: NaiveDate) -> u32 {
    if d < start {
        return 0;
    }
    let mut y = d.year() - start.year();
    if (d.month(), d.day()) < (start.month(), start.day()) {
        y -= 1;
    }
    y.max(0) as u32
}

/// Work anniversary in a given year (29 Feb falls on 1 Mar in non-leap years).
pub fn anniversary(start: NaiveDate, year: i32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, start.month(), start.day())
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(year, 3, 1).unwrap())
}

/// The annual entitlement (days per year) that applies on `d`.
pub fn entitlement_on(s: &Settings, start: NaiveDate, d: NaiveDate) -> f64 {
    let e = s.base_days + s.increment_per_year * years_of_service(start, d) as f64;
    match s.max_entitlement {
        Some(m) => e.min(m),
        None => e,
    }
}

/// The annual rate that accrues on `d`, honouring when increases take effect.
pub fn rate_on(s: &Settings, start: NaiveDate, d: NaiveDate) -> f64 {
    match s.rate_basis {
        RateBasis::Anniversary => entitlement_on(s, start, d),
        RateBasis::CycleStart => {
            let (cs, _) = cycle_bounds(s, start, d);
            entitlement_on(s, start, cs.max(start))
        }
    }
}

/// First and last day of the leave cycle containing `d`.
pub fn cycle_bounds(s: &Settings, start: NaiveDate, d: NaiveDate) -> (NaiveDate, NaiveDate) {
    match s.cycle {
        CycleBasis::Calendar => (
            NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(d.year(), 12, 31).unwrap(),
        ),
        CycleBasis::Anniversary => {
            let mut a = anniversary(start, d.year());
            if a > d {
                a = anniversary(start, d.year() - 1);
            }
            let next = anniversary(start, a.year() + 1);
            (a.max(start), next - Days::new(1))
        }
    }
}

/// Days charged for an entry, and the working dates they fall on.
pub fn entry_days(cal: &WorkCalendar, e: &LeaveEntry) -> (f64, Vec<NaiveDate>) {
    let dates = if e.end < e.start {
        Vec::new()
    } else {
        cal.working_days(e.start, e.end)
    };
    let days = match e.days_override {
        Some(v) => v,
        None if e.half_day && dates.len() == 1 => 0.5,
        None => dates.len() as f64,
    };
    (days, dates)
}

#[derive(Clone, Debug, PartialEq)]
pub struct CarriedLot {
    pub amount: f64,
    /// Last day the days may be used. `None` = never expires.
    pub use_by: Option<NaiveDate>,
    pub from_cycle_end: NaiveDate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LossReason {
    CarryCap,
    Expired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Loss {
    pub date: NaiveDate,
    pub days: f64,
    pub reason: LossReason,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonthRow {
    pub year: i32,
    pub month: u32,
    pub opening: f64,
    pub accrued: f64,
    pub taken: f64,
    pub adjusted: f64,
    pub lost: f64,
    pub closing: f64,
    /// Annual entitlement in force at the end of the month.
    pub rate: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CycleRow {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub opening: f64,
    pub accrued: f64,
    pub taken: f64,
    pub adjusted: f64,
    pub lost: f64,
    pub closing: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdjustmentEffect {
    pub id: u64,
    /// The actual change applied (for SetBalance this is the computed difference).
    pub delta: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub start: NaiveDate,
    pub today: NaiveDate,
    pub horizon: NaiveDate,
    /// Credited balance at end of each day, `start..=horizon`.
    pub balances: Vec<f64>,
    /// Accrued-but-not-yet-credited at end of each day (month-end crediting only).
    pub pending: Vec<f64>,
    /// Cumulative credited accrual at end of each day.
    pub accrued_cum: Vec<f64>,
    pub months: Vec<MonthRow>,
    pub cycles: Vec<CycleRow>,
    pub losses: Vec<Loss>,
    pub adjustments: Vec<AdjustmentEffect>,
    /// Days charged per entry id (all kinds).
    pub entry_days: HashMap<u64, f64>,
    /// Carried-over lots still alive at the end of today.
    pub carried_today: Vec<CarriedLot>,
    pub calendar: WorkCalendar,
}

impl Report {
    fn idx(&self, d: NaiveDate) -> Option<usize> {
        if d < self.start || d > self.horizon {
            return None;
        }
        Some((d - self.start).num_days() as usize)
    }

    /// Credited balance at the end of `d` (0 before start; clamps to horizon).
    pub fn balance_on(&self, d: NaiveDate) -> f64 {
        if d < self.start {
            return 0.0;
        }
        let i = self.idx(d.min(self.horizon)).unwrap();
        self.balances[i]
    }

    pub fn pending_on(&self, d: NaiveDate) -> f64 {
        self.idx(d).map(|i| self.pending[i]).unwrap_or(0.0)
    }

    pub fn balance_today(&self) -> f64 {
        self.balance_on(self.today)
    }

    /// Accrual credited in the inclusive range.
    pub fn accrued_between(&self, a: NaiveDate, b: NaiveDate) -> f64 {
        let cum = |d: NaiveDate| {
            if d < self.start {
                0.0
            } else {
                self.accrued_cum[self.idx(d.min(self.horizon)).unwrap()]
            }
        };
        cum(b) - cum(a - Days::new(1))
    }

    /// Annual leave days falling in the inclusive range.
    pub fn taken_between(&self, data: &Data, a: NaiveDate, b: NaiveDate) -> f64 {
        data.entries
            .iter()
            .filter(|e| e.kind.deducts() && e.end >= a && e.start <= b)
            .map(|e| {
                let (days, dates) = entry_days(&self.calendar, e);
                if dates.is_empty() {
                    return if e.start >= a && e.start <= b {
                        days
                    } else {
                        0.0
                    };
                }
                let per = days / dates.len() as f64;
                per * dates.iter().filter(|d| **d >= a && **d <= b).count() as f64
            })
            .sum()
    }
}

struct Ledger {
    current: f64,
    carried: Vec<CarriedLot>,
    deficit: f64,
}

impl Ledger {
    fn balance(&self) -> f64 {
        self.current + self.carried.iter().map(|l| l.amount).sum::<f64>() - self.deficit
    }

    fn credit(&mut self, mut amt: f64) {
        if self.deficit > 0.0 {
            let pay = amt.min(self.deficit);
            self.deficit -= pay;
            amt -= pay;
        }
        self.current += amt;
    }

    /// Consume soonest-expiring carried days first, then never-expiring carried, then current.
    fn debit(&mut self, mut amt: f64) -> f64 {
        let taken = amt;
        self.carried.sort_by(|a, b| match (a.use_by, b.use_by) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.from_cycle_end.cmp(&b.from_cycle_end),
        });
        for lot in self.carried.iter_mut() {
            if amt <= EPS {
                break;
            }
            let use_ = lot.amount.min(amt);
            lot.amount -= use_;
            amt -= use_;
        }
        self.carried.retain(|l| l.amount > EPS);
        let use_ = self.current.max(0.0).min(amt);
        self.current -= use_;
        amt -= use_;
        self.deficit += amt;
        taken
    }
}

/// Replays the ledger. Returns `None` until a start date is set.
pub fn compute(data: &Data, today: NaiveDate, extend_to: Option<NaiveDate>) -> Option<Report> {
    let s = &data.settings;
    let start = s.start_date?;

    let last_entry = data.entries.iter().map(|e| e.end).max().unwrap_or(start);
    let last_adj = data
        .adjustments
        .iter()
        .map(|a| a.date)
        .max()
        .unwrap_or(start);
    let base = today.max(start);
    let horizon = [
        base + Months::new(24),
        cycle_bounds(s, start, base + Months::new(12)).1,
        last_entry,
        last_adj,
        extend_to.unwrap_or(start),
    ]
    .into_iter()
    .max()
    .unwrap();

    let calendar = WorkCalendar::new(
        s,
        start.year().min(last_entry.year()) - 1,
        horizon.year() + 1,
    );
    let n = (horizon - start).num_days() as usize + 1;

    // Per-day debits from annual leave.
    let mut debits = vec![0.0f64; n];
    let mut entry_days_map = HashMap::with_capacity(data.entries.len());
    for e in &data.entries {
        let (days, dates) = entry_days(&calendar, e);
        entry_days_map.insert(e.id, days);
        if !e.kind.deducts() || days.abs() < EPS {
            continue;
        }
        let in_range: Vec<_> = dates.iter().filter(|d| **d >= start).collect();
        if in_range.is_empty() {
            if e.start >= start {
                debits[(e.start - start).num_days() as usize] += days;
            }
            continue;
        }
        let per = days / dates.len() as f64;
        for d in in_range {
            debits[(*d - start).num_days() as usize] += per;
        }
    }

    // Adjustments grouped per day, preserving entry order.
    let mut adj_by_day: HashMap<usize, Vec<(u64, AdjustmentKind)>> = HashMap::new();
    for a in &data.adjustments {
        let i = if a.date < start {
            0
        } else {
            (a.date - start).num_days() as usize
        };
        adj_by_day.entry(i).or_default().push((a.id, a.kind));
    }

    let mut ledger = Ledger {
        current: 0.0,
        carried: Vec::new(),
        deficit: 0.0,
    };
    let mut pending = 0.0;
    let mut balances = Vec::with_capacity(n);
    let mut pendings = Vec::with_capacity(n);
    let mut accrued_cum = Vec::with_capacity(n);
    let mut total_accrued = 0.0;
    let mut months: Vec<MonthRow> = Vec::new();
    let mut cycles: Vec<CycleRow> = Vec::new();
    let mut losses = Vec::new();
    let mut adjustments = Vec::new();
    let mut carried_today = Vec::new();

    let mut cycle_end = cycle_bounds(s, start, start).1;
    cycles.push(CycleRow {
        start,
        end: cycle_end,
        ..Default::default()
    });

    for (i, d) in start.iter_days().take(n).enumerate() {
        if d.day() == 1 || i == 0 {
            let b = ledger.balance();
            months.push(MonthRow {
                year: d.year(),
                month: d.month(),
                opening: b,
                ..Default::default()
            });
        }
        if i > 0 && d == cycle_end + Days::new(1) {
            cycle_end = cycle_bounds(s, start, d).1;
            cycles.push(CycleRow {
                start: d,
                end: cycle_end,
                opening: ledger.balance(),
                ..Default::default()
            });
        }
        let month = months.last_mut().unwrap();
        let cycle = cycles.last_mut().unwrap();
        let cycle_start = cycle.start;

        // 1. Expiry.
        let mut expired = 0.0;
        ledger.carried.retain(|l| match l.use_by {
            Some(u) if d > u => {
                expired += l.amount;
                false
            }
            _ => true,
        });
        if expired > EPS {
            losses.push(Loss {
                date: d,
                days: expired,
                reason: LossReason::Expired,
            });
            month.lost += expired;
            cycle.lost += expired;
        }

        // 2. Accrual.
        let rate = rate_on(s, start, d);
        let month_end = d.day() == days_in_month(d);
        if s.credit_timing == CreditTiming::Upfront {
            if d == cycle_start {
                let (full_start, full_end) = cycle_bounds(s, start, d);
                let full = (full_end - full_start).num_days() as f64 + 1.0;
                let part = (full_end - d).num_days() as f64 + 1.0;
                pending += rate * (part / full).min(1.0);
            }
        } else {
            pending += rate / 12.0 / days_in_month(d) as f64;
        }
        if s.credit_timing != CreditTiming::MonthEnd || month_end || d == cycle_end {
            ledger.credit(pending);
            total_accrued += pending;
            month.accrued += pending;
            cycle.accrued += pending;
            pending = 0.0;
        }
        month.rate = rate;

        // 3. Adjustments.
        if let Some(list) = adj_by_day.get(&i) {
            for (id, kind) in list {
                let delta = match *kind {
                    AdjustmentKind::Delta(v) => v,
                    AdjustmentKind::SetBalance(v) => v - ledger.balance(),
                };
                if delta >= 0.0 {
                    ledger.credit(delta);
                } else {
                    ledger.debit(-delta);
                }
                month.adjusted += delta;
                cycle.adjusted += delta;
                adjustments.push(AdjustmentEffect { id: *id, delta });
            }
        }

        // 4. Leave taken.
        if debits[i] > 0.0 {
            let t = ledger.debit(debits[i]);
            month.taken += t;
            cycle.taken += t;
        } else if debits[i] < 0.0 {
            // A negative override behaves as leave given back.
            ledger.credit(-debits[i]);
            month.taken += debits[i];
            cycle.taken += debits[i];
        }

        // 5. Cycle close: cap, then carry over.
        if d == cycle_end {
            if let Some(cap) = s.carry_cap {
                let positive =
                    ledger.current.max(0.0) + ledger.carried.iter().map(|l| l.amount).sum::<f64>();
                let excess = positive - cap.max(0.0);
                if excess > EPS {
                    // Forfeit the oldest days first so the most recent are kept.
                    ledger.debit(excess);
                    losses.push(Loss {
                        date: d,
                        days: excess,
                        reason: LossReason::CarryCap,
                    });
                    month.lost += excess;
                    cycle.lost += excess;
                }
            }
            if ledger.current > EPS {
                let use_by = s
                    .carry_expiry_months
                    .map(|m| (d + Days::new(1)) + Months::new(m) - Days::new(1));
                ledger.carried.push(CarriedLot {
                    amount: ledger.current,
                    use_by,
                    from_cycle_end: d,
                });
            }
            ledger.current = 0.0;
        }

        let b = ledger.balance();
        month.closing = b;
        cycle.closing = b;
        balances.push(b);
        pendings.push(pending);
        accrued_cum.push(total_accrued);
        if d == today {
            carried_today = ledger.carried.clone();
        }
    }

    Some(Report {
        start,
        today,
        horizon,
        balances,
        pending: pendings,
        accrued_cum,
        months,
        cycles,
        losses,
        adjustments,
        entry_days: entry_days_map,
        carried_today,
        calendar,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn data(start: NaiveDate) -> Data {
        let mut x = Data::default();
        x.settings.start_date = Some(start);
        x
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    #[test]
    fn service_years_and_entitlement() {
        let s = Settings::default();
        let st = d(2020, 3, 15);
        assert_eq!(years_of_service(st, d(2021, 3, 14)), 0);
        assert_eq!(years_of_service(st, d(2021, 3, 15)), 1);
        close(entitlement_on(&s, st, d(2025, 3, 15)), 20.0);
        let capped = Settings {
            max_entitlement: Some(18.0),
            ..s
        };
        close(entitlement_on(&capped, st, d(2025, 3, 15)), 18.0);
    }

    #[test]
    fn full_month_accrues_one_twelfth() {
        let x = data(d(2025, 1, 1));
        let r = compute(&x, d(2025, 12, 31), None).unwrap();
        close(r.balance_on(d(2025, 1, 30)), 0.0);
        close(r.balance_on(d(2025, 1, 31)), 1.25);
        close(r.balance_on(d(2025, 12, 31)), 15.0);
        close(r.pending_on(d(2025, 2, 14)), 1.25 * 14.0 / 28.0);
    }

    #[test]
    fn partial_first_month_is_prorated_by_calendar_days() {
        let x = data(d(2025, 1, 17)); // 15 of 31 days
        let r = compute(&x, d(2025, 2, 1), None).unwrap();
        close(r.balance_on(d(2025, 1, 31)), 1.25 * 15.0 / 31.0);
    }

    #[test]
    fn anniversary_bumps_rate_mid_month() {
        let mut x = data(d(2024, 6, 16));
        x.settings.cycle = CycleBasis::Calendar;
        let r = compute(&x, d(2025, 7, 1), None).unwrap();
        let june = r
            .months
            .iter()
            .find(|m| m.year == 2025 && m.month == 6)
            .unwrap();
        // 15 days at 15/yr and 15 days at 16/yr.
        close(june.accrued, 15.0 / 12.0 * 0.5 + 16.0 / 12.0 * 0.5);
        close(june.rate, 16.0);
    }

    #[test]
    fn full_year_totals_per_cycle() {
        let x = data(d(2020, 3, 1));
        let r = compute(&x, d(2026, 1, 1), None).unwrap();
        let c = &r.cycles;
        close(c[0].accrued, 15.0);
        close(c[1].accrued, 16.0);
        close(c[4].accrued, 19.0);
        assert_eq!(c[1].start, d(2021, 3, 1));
        assert_eq!(c[1].end, d(2022, 2, 28));
    }

    #[test]
    fn leave_deducts_working_days_only() {
        let mut x = data(d(2025, 1, 1));
        let id = x.take_id();
        x.entries.push(LeaveEntry {
            id,
            kind: LeaveKind::Annual,
            start: d(2025, 9, 22),
            end: d(2025, 9, 28),
            half_day: false,
            days_override: None,
            note: String::new(),
        });
        let sick = x.take_id();
        x.entries.push(LeaveEntry {
            id: sick,
            kind: LeaveKind::Sick,
            start: d(2025, 10, 6),
            end: d(2025, 10, 6),
            half_day: false,
            days_override: None,
            note: String::new(),
        });
        let r = compute(&x, d(2025, 12, 31), None).unwrap();
        close(r.entry_days[&id], 4.0);
        close(r.entry_days[&sick], 1.0);
        close(r.balance_on(d(2025, 12, 31)), 11.0);
    }

    #[test]
    fn half_day_and_override() {
        let mut x = data(d(2025, 1, 1));
        x.entries.push(LeaveEntry {
            id: 1,
            kind: LeaveKind::Annual,
            start: d(2025, 2, 3),
            end: d(2025, 2, 3),
            half_day: true,
            days_override: None,
            note: String::new(),
        });
        x.entries.push(LeaveEntry {
            id: 2,
            kind: LeaveKind::Annual,
            start: d(2025, 3, 3),
            end: d(2025, 3, 7),
            half_day: false,
            days_override: Some(2.5),
            note: String::new(),
        });
        x.next_id = 3;
        let r = compute(&x, d(2025, 12, 31), None).unwrap();
        close(r.balance_on(d(2025, 12, 31)), 12.0);
    }

    #[test]
    fn set_balance_adjustment() {
        let mut x = data(d(2020, 1, 1));
        x.adjustments.push(Adjustment {
            id: 1,
            date: d(2024, 12, 31),
            kind: AdjustmentKind::SetBalance(7.5),
            note: "HR confirmed".into(),
        });
        x.settings.cycle = CycleBasis::Calendar;
        let r = compute(&x, d(2025, 1, 31), None).unwrap();
        close(r.balance_on(d(2024, 12, 31)), 7.5);
        close(r.balance_on(d(2025, 1, 31)), 7.5 + 20.0 / 12.0);
    }

    #[test]
    fn carry_cap_forfeits_excess() {
        let mut x = data(d(2024, 1, 1));
        x.settings.cycle = CycleBasis::Calendar;
        x.settings.carry_cap = Some(5.0);
        let r = compute(&x, d(2025, 1, 1), None).unwrap();
        close(r.balance_on(d(2024, 12, 31)), 5.0);
        assert_eq!(r.losses[0].date, d(2024, 12, 31));
        close(r.losses[0].days, 10.0);
        assert_eq!(r.losses[0].reason, LossReason::CarryCap);
    }

    #[test]
    fn carried_days_expire_and_are_used_first() {
        let mut x = data(d(2024, 1, 1));
        x.settings.cycle = CycleBasis::Calendar;
        x.settings.carry_expiry_months = Some(6);
        // 3 days taken in March 2025 come out of the 15 carried days.
        x.entries.push(LeaveEntry {
            id: 1,
            kind: LeaveKind::Annual,
            start: d(2025, 3, 3),
            end: d(2025, 3, 5),
            half_day: false,
            days_override: None,
            note: String::new(),
        });
        x.next_id = 2;
        let r = compute(&x, d(2025, 3, 10), None).unwrap();
        assert_eq!(r.carried_today[0].use_by, Some(d(2025, 6, 30)));
        close(r.carried_today[0].amount, 12.0);
        let exp = r
            .losses
            .iter()
            .find(|l| l.reason == LossReason::Expired)
            .unwrap();
        assert_eq!(exp.date, d(2025, 7, 1));
        close(exp.days, 12.0);
        // 6 months of 2025 accrual (16 days/yr in year two) remain after expiry.
        close(r.balance_on(d(2025, 7, 1)), 8.0);
    }

    #[test]
    fn negative_balance_is_repaid_by_accrual() {
        let mut x = data(d(2025, 1, 1));
        x.entries.push(LeaveEntry {
            id: 1,
            kind: LeaveKind::Annual,
            start: d(2025, 1, 6),
            end: d(2025, 1, 10),
            half_day: false,
            days_override: None,
            note: String::new(),
        });
        x.next_id = 2;
        let r = compute(&x, d(2025, 6, 1), None).unwrap();
        close(r.balance_on(d(2025, 1, 10)), -5.0);
        close(r.balance_on(d(2025, 4, 30)), 0.0);
    }

    #[test]
    fn upfront_calendar_allowance_matches_hr_style() {
        // Joined Dec 2023; HR: "17 days allowance for 2026".
        let mut x = data(d(2023, 12, 4));
        x.settings.cycle = CycleBasis::Calendar;
        x.settings.credit_timing = CreditTiming::Upfront;
        x.settings.rate_basis = RateBasis::CycleStart;
        let r = compute(&x, d(2026, 9, 7), None).unwrap();
        // Partial first cycle: 28 of 31 December days.
        close(r.cycles[0].accrued, 15.0 * 28.0 / 365.0);
        close(r.cycles[1].accrued, 15.0);
        close(r.cycles[2].accrued, 16.0);
        close(r.cycles[3].accrued, 17.0);
        close(r.accrued_between(d(2026, 1, 1), d(2026, 1, 1)), 17.0);
    }

    #[test]
    fn rate_basis_cycle_start_delays_increase() {
        let mut x = data(d(2023, 12, 4));
        x.settings.cycle = CycleBasis::Calendar;
        x.settings.rate_basis = RateBasis::CycleStart;
        let r = compute(&x, d(2026, 9, 7), None).unwrap();
        close(r.cycles[2].accrued, 16.0); // 2025: no mid-December bump
        x.settings.rate_basis = RateBasis::Anniversary;
        let r = compute(&x, d(2026, 9, 7), None).unwrap();
        assert!(r.cycles[2].accrued > 16.0);
    }

    #[test]
    fn hr_email_scenario_2026() {
        // HR (7 Sep 2026): 17 days for 2026; 1.5 carried from 2025 used on 2 Jan and
        // 2 Mar (half); 11 days taken/booked from the 2026 allowance, incl. 24 Sep.
        let mut x = data(d(2023, 12, 4));
        x.settings.cycle = CycleBasis::Calendar;
        x.settings.credit_timing = CreditTiming::Upfront;
        x.settings.rate_basis = RateBasis::CycleStart;
        x.adjustments.push(Adjustment {
            id: 1,
            date: d(2025, 12, 31),
            kind: AdjustmentKind::SetBalance(1.5),
            note: String::new(),
        });
        let mut id = 2;
        let mut add = |a: NaiveDate, b: NaiveDate, half: bool| {
            x.entries.push(LeaveEntry {
                id,
                kind: LeaveKind::Annual,
                start: a,
                end: b,
                half_day: half,
                days_override: None,
                note: String::new(),
            });
            id += 1;
        };
        add(d(2026, 1, 2), d(2026, 1, 2), false);
        add(d(2026, 3, 2), d(2026, 3, 2), true);
        for day in [15, 18, 19, 26] {
            add(d(2026, 6, day), d(2026, 6, day), false);
        }
        add(d(2026, 7, 3), d(2026, 7, 3), false);
        add(d(2026, 7, 27), d(2026, 7, 31), false);
        add(d(2026, 9, 24), d(2026, 9, 24), false); // Heritage Day
        x.next_id = id;
        let r = compute(&x, d(2026, 9, 7), None).unwrap();
        close(r.balance_today(), 7.0);
        // 24 Sep is a public holiday, so it costs nothing: HR's 6 is one day short.
        close(r.entry_days[&(id - 1)], 0.0);
        close(r.balance_on(d(2026, 12, 31)), 7.0);
    }

    #[test]
    fn json_roundtrip() {
        let mut x = data(d(2021, 5, 4));
        x.adjustments.push(Adjustment {
            id: 9,
            date: d(2022, 1, 1),
            kind: AdjustmentKind::Delta(-1.0),
            note: String::new(),
        });
        let y = Data::from_json(&x.to_json()).unwrap();
        assert_eq!(y.settings, x.settings);
        assert_eq!(y.next_id, 10);
    }
}
