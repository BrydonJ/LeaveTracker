//! Home: balance hero, key numbers, alerts, balance chart and planning.

use chrono::{Datelike, Days, Months, NaiveDate};
use leave_core::engine::{anniversary, cycle_bounds, rate_on, years_of_service, LossReason};
use leave_core::CreditTiming;
use leptos::prelude::*;

use crate::chart::BalanceChart;
use crate::fmt;
use crate::leave::EntryRow;
use crate::ui::{Counter, Icon};
use crate::{store, use_app, Tab};

#[derive(Clone, Debug, PartialEq)]
struct Summary {
    balance: f64,
    pending: f64,
    next_credit: NaiveDate,
    rate: f64,
    years: u32,
    next_anniv: NaiveDate,
    next_rate: f64,
    cycle_start: NaiveDate,
    cycle_end: NaiveDate,
    earned_cycle: f64,
    taken_cycle: f64,
    booked: f64,
    month_end_mode: bool,
    upfront_mode: bool,
    next_cycle_credit: f64,
    not_started: bool,
    cycle: Option<leave_core::engine::CycleRow>,
    allowance: f64,
    booked_cycle: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct Alert {
    tone: &'static str,
    icon: &'static str,
    title: String,
    body: String,
}

#[component]
pub fn Overview() -> impl IntoView {
    let app = use_app();

    let summary = Memo::new(move |_| {
        let r = app.report.get()?;
        let today = app.today.get();
        app.data.with(|d| {
            let s = &d.settings;
            let start = r.start;
            let (cs, ce) = cycle_bounds(s, start, today.max(start));
            let month_end = leave_core::calendar::last_of_month(today);
            let mut next_anniv = anniversary(start, today.year());
            if next_anniv <= today {
                next_anniv = anniversary(start, today.year() + 1);
            }
            // The increase lands on the anniversary or at the following cycle start.
            let rate_now = rate_on(s, start, today);
            let next_change = [
                next_anniv,
                cycle_bounds(s, start, next_anniv).1 + Days::new(1),
            ]
            .into_iter()
            .find(|c| rate_on(s, start, *c) > rate_now + 1e-9)
            .unwrap_or(next_anniv);
            let booked = r.taken_between(d, today + Days::new(1), NaiveDate::MAX - Days::new(1));
            Some(Summary {
                balance: r.balance_today(),
                pending: r.pending_on(today),
                next_credit: month_end.min(ce),
                rate: rate_now,
                years: years_of_service(start, today),
                next_anniv: next_change,
                next_rate: rate_on(s, start, next_change),
                cycle_start: cs,
                cycle_end: ce,
                earned_cycle: r.accrued_between(cs, today),
                taken_cycle: r.taken_between(d, cs, today),
                booked,
                month_end_mode: s.credit_timing == CreditTiming::MonthEnd,
                upfront_mode: s.credit_timing == CreditTiming::Upfront,
                next_cycle_credit: r.accrued_between(ce + Days::new(1), ce + Days::new(1)),
                cycle: r
                    .cycles
                    .iter()
                    .find(|c| c.start <= today && c.end >= today)
                    .cloned(),
                allowance: r.accrued_between(cs, ce),
                booked_cycle: r.taken_between(d, today + Days::new(1), ce),
                not_started: today < start,
            })
        })
    });

    let alerts = Memo::new(move |_| {
        let Some(r) = app.report.get() else {
            return Vec::new();
        };
        let today = app.today.get();
        let mut out = Vec::new();
        for lot in &r.carried_today {
            if let Some(u) = lot.use_by {
                if lot.amount > 0.004 {
                    out.push(Alert {
                        tone: if (u - today).num_days() <= 60 {
                            "serious"
                        } else {
                            "warning"
                        },
                        icon: "clock",
                        title: format!(
                            "{} carried over — use by {}",
                            fmt::plural_days(lot.amount),
                            fmt::date_med(u)
                        ),
                        body: format!(
                            "These days roll off {}. Leave you take is drawn from them first.",
                            fmt::relative(u + Days::new(1), today)
                        ),
                    });
                }
            }
        }
        if let Some(loss) = r.losses.iter().find(|l| l.date >= today && l.days > 0.004) {
            let (title, body) = match loss.reason {
                LossReason::CarryCap => (
                    format!("{} will be forfeited on {}", fmt::plural_days(loss.days), fmt::date_med(loss.date)),
                    "Your projected balance at cycle end is above the carry-over cap. Book leave before then to keep it."
                        .to_string(),
                ),
                LossReason::Expired => (
                    format!("{} will expire on {}", fmt::plural_days(loss.days), fmt::date_med(loss.date)),
                    "Carried-over days that aren't used by then are lost.".to_string(),
                ),
            };
            out.push(Alert {
                tone: "critical",
                icon: "alert",
                title,
                body,
            });
        }
        out
    });

    let backup_due = move || {
        let has = app
            .data
            .with(|d| !d.entries.is_empty() || !d.adjustments.is_empty());
        if !has {
            return false;
        }
        match (app.last_backup.get(), store::last_change()) {
            (None, _) => true,
            (Some(b), Some(c)) => c > b && store::now_ms() - b > 14.0 * 86_400_000.0,
            _ => false,
        }
    };

    let upcoming = Memo::new(move |_| {
        let today = app.today.get();
        let r = app.report.get();
        app.data.with(|d| {
            let mut v: Vec<_> = d
                .entries
                .iter()
                .filter(|e| e.end >= today)
                .map(|e| {
                    (
                        e.clone(),
                        r.as_ref()
                            .and_then(|r| r.entry_days.get(&e.id).copied())
                            .unwrap_or(0.0),
                    )
                })
                .collect();
            v.sort_by_key(|x| x.0.start);
            v.truncate(4);
            v
        })
    });

    view! {
        {move || summary.get().map(|s| view! { <Hero s=s/> })}

        <Show when=backup_due>
            <div class="alert tone-info reveal">
                <span class="alert-icon"><Icon name="shield"/></span>
                <div class="alert-text">
                    <strong>"Back up your leave record"</strong>
                    <span>"It's stored only in this browser. Export a copy so it can never be lost."</span>
                </div>
                <button class="btn small" on:click=move |_| crate::settings::export_json(app)>
                    <Icon name="download"/>"Export"
                </button>
            </div>
        </Show>

        <For
            each=move || alerts.get().into_iter().enumerate()
            key=|(_, a)| a.title.clone()
            children=move |(i, a)| view! {
                <div class=format!("alert reveal tone-{}", a.tone) style=format!("--i:{i}")>
                    <span class="alert-icon"><Icon name=a.icon/></span>
                    <div class="alert-text">
                        <strong>{a.title}</strong>
                        <span>{a.body}</span>
                    </div>
                </div>
            }
        />

        <div class="grid-2">
            <section class="card chart-card reveal" style="--i:2">
                <BalanceChart/>
            </section>
            <div class="stack">
                {move || summary.get().map(|s| view! { <CycleCard s=s/> })}
                <PlanAhead/>
                <section class="card reveal" style="--i:4">
                    <header class="card-head">
                        <h3>"Coming up"</h3>
                        <button class="link" on:click=move |_| app.tab.set(Tab::Leave)>"All leave"<Icon name="chev-right"/></button>
                    </header>
                    {move || {
                        let today = app.today.get();
                        let items = upcoming.get();
                        if items.is_empty() {
                            view! { <p class="muted small-pad">"Nothing booked. You've earned a break."</p> }.into_any()
                        } else {
                            view! {
                                <div class="list compact">
                                    {items.into_iter().enumerate().map(|(i, (e, d))| view! { <EntryRow entry=e days=d today=today index=i/> }).collect_view()}
                                </div>
                            }.into_any()
                        }
                    }}
                </section>
            </div>
        </div>
    }
}

#[component]
fn Hero(s: Summary) -> impl IntoView {
    let app = use_app();
    let balance =
        Signal::derive(move || app.report.get().map(|r| r.balance_today()).unwrap_or(0.0));
    let after_booked = s.balance - s.booked;
    let at_max = (s.next_rate - s.rate).abs() < 1e-9;
    let neg = s.balance < -0.004;

    view! {
        <section class="hero reveal" class:neg=neg>
            <div class="hero-main">
                <div class="hero-label">
                    {if s.not_started { "Starts accruing ".to_string() + &fmt::date_med(s.cycle_start) } else { "Available today".into() }}
                </div>
                <div class="hero-value">
                    <Counter value=balance/>
                    <span class="hero-unit">"days"</span>
                </div>
                <div class="hero-chips">
                    {(s.month_end_mode && s.pending > 0.004).then(|| view! {
                        <span class="pill">
                            <Icon name="trend"/>
                            {format!("+{} accruing · credited {}", fmt::days(s.pending), fmt::date_short(s.next_credit))}
                        </span>
                    })}
                    {(s.upfront_mode && !s.not_started).then(|| view! {
                        <span class="pill">
                            <Icon name="trend"/>
                            {format!("+{} credited {}", fmt::days(s.next_cycle_credit), fmt::date_med(s.cycle_end + Days::new(1)))}
                        </span>
                    })}
                    {(s.booked > 0.004).then(|| view! {
                        <span class="pill">
                            <Icon name="calendar"/>
                            {format!("{} after booked leave", fmt::days(after_booked))}
                        </span>
                    })}
                </div>
            </div>
            <div class="hero-stats">
                <div class="stat">
                    <span class="stat-label">"Entitlement"</span>
                    <span class="stat-value">{fmt::days(s.rate)}<small>" days / yr"</small></span>
                    <span class="stat-sub">{format!("{} year of service", fmt::ordinal(s.years + 1))}</span>
                </div>
                <div class="stat">
                    <span class="stat-label">"Earned this cycle"</span>
                    <span class="stat-value">{fmt::days(s.earned_cycle)}</span>
                    <span class="stat-sub">{format!("since {}", fmt::date_med(s.cycle_start))}</span>
                </div>
                <div class="stat">
                    <span class="stat-label">"Taken this cycle"</span>
                    <span class="stat-value">{fmt::days(s.taken_cycle)}</span>
                    <span class="stat-sub">{format!("cycle ends {}", fmt::date_med(s.cycle_end))}</span>
                </div>
                <div class="stat">
                    <span class="stat-label">{if at_max { "Next anniversary" } else { "Next increase" }}</span>
                    <span class="stat-value">
                        {if at_max { fmt::date_short(s.next_anniv) } else { fmt::days(s.next_rate) }}
                        {(!at_max).then(|| view! { <small>" days / yr"</small> })}
                    </span>
                    <span class="stat-sub">
                        {if at_max { "entitlement is at its cap".to_string() } else { format!("{} · {}", fmt::date_med(s.next_anniv), fmt::relative(s.next_anniv, app.today.get_untracked())) }}
                    </span>
                </div>
            </div>
        </section>
    }
}

/// The cycle the way HR reports it: carried in + allowance − taken − booked.
#[component]
fn CycleCard(s: Summary) -> impl IntoView {
    let Some(c) = s.cycle.clone() else {
        return ().into_any();
    };
    let carried = c.opening;
    let other = c.adjusted - c.lost;
    let end_balance = c.closing;
    let total = (carried.max(0.0) + s.allowance + other.max(0.0)).max(0.0001);
    let pct = |v: f64| (v.max(0.0) / total * 100.0).min(100.0);
    let taken_w = pct(s.taken_cycle);
    let booked_w = pct(s.booked_cycle);
    let left_w = pct(end_balance);
    let label = if c.start.month() == 1 && c.start.day() == 1 && c.end.month() == 12 {
        format!("{} leave cycle", c.start.year())
    } else {
        format!(
            "Cycle {} – {}",
            fmt::date_short(c.start),
            fmt::date_med(c.end)
        )
    };
    view! {
        <section class="card cycle-card reveal" style="--i:2">
            <header class="card-head">
                <h3>{label}</h3>
                <span class="card-sub">{format!("ends {}", fmt::date_med(c.end))}</span>
            </header>
            <div class="meter" role="img" aria-label=format!(
                "{} taken, {} booked, {} left at cycle end",
                fmt::days(s.taken_cycle), fmt::days(s.booked_cycle), fmt::days(end_balance)
            )>
                <span class="seg taken" style=format!("--w:{taken_w}%")></span>
                <span class="seg booked" style=format!("--w:{booked_w}%")></span>
                <span class="seg left" style=format!("--w:{left_w}%")></span>
            </div>
            <dl class="sum-rows">
                <div><dt>"Carried in"</dt><dd>{fmt::days(carried)}</dd></div>
                <div><dt>{if s.month_end_mode { "Allowance (accrues monthly)" } else { "Allowance" }}</dt><dd>{format!("+{}", fmt::days(s.allowance))}</dd></div>
                <div><dt><span class="key taken"></span>"Taken"</dt><dd>{fmt::days(-s.taken_cycle)}</dd></div>
                <div><dt><span class="key booked"></span>"Booked"</dt><dd>{fmt::days(-s.booked_cycle)}</dd></div>
                {(other.abs() > 0.004).then(|| view! {
                    <div><dt>"Adjustments & forfeits"</dt><dd>{fmt::days_signed(other)}</dd></div>
                })}
                <div class="total"><dt><span class="key left"></span>"Left at cycle end"</dt><dd>{fmt::days(end_balance)}</dd></div>
            </dl>
        </section>
    }
    .into_any()
}

#[component]
fn PlanAhead() -> impl IntoView {
    let app = use_app();
    let default = app
        .today
        .get_untracked()
        .checked_add_months(Months::new(3))
        .unwrap_or(app.today.get_untracked());
    let date = RwSignal::new(fmt::iso(default));
    let result = Memo::new(move |_| {
        let d = fmt::parse_iso(&date.get())?;
        let r = app.report.get()?;
        if d > r.horizon {
            // Extend the projection to the requested day.
            let today = app.today.get();
            let r2 = app
                .data
                .with(|data| leave_core::compute(data, today, Some(d)))?;
            return Some((d, r2.balance_on(d), r2.pending_on(d)));
        }
        Some((d, r.balance_on(d), r.pending_on(d)))
    });
    view! {
        <section class="card plan reveal" style="--i:3">
            <header class="card-head">
                <h3>"Plan ahead"</h3>
            </header>
            <label class="field inline">
                <span>"Balance on"</span>
                <input type="date" prop:value=date on:input=move |e| date.set(event_target_value(&e))/>
            </label>
            {move || result.get().map(|(d, b, p)| view! {
                <div class="plan-result">
                    <span class="plan-num" class:neg=b < -0.004>{fmt::days(b)}</span>
                    <span class="plan-text">
                        {format!("days on {}", fmt::date_long(d))}
                        <small>
                            {if p > 0.004 { format!("plus {} accrued, credited at month end. ", fmt::days(p)) } else { String::new() }}
                            "Includes booked leave and future accrual."
                        </small>
                    </span>
                </div>
            })}
        </section>
    }
}
