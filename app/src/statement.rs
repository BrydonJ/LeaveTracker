//! Statement: per-cycle and per-month ledger, HR reconciliation and adjustments.

use chrono::{Datelike, NaiveDate};
use leave_core::engine::{CycleRow, MonthRow};
use leave_core::{Adjustment, AdjustmentKind, CreditTiming};
use leptos::prelude::*;

use crate::fmt;
use crate::ui::{parse_num, Icon, Segmented};
use crate::use_app;

#[component]
pub fn Statement() -> impl IntoView {
    let app = use_app();
    let show_future = RwSignal::new(false);

    let cycles = Memo::new(move |_| {
        let Some(r) = app.report.get() else {
            return Vec::new();
        };
        let today = app.today.get();
        let future = show_future.get();
        let mut out: Vec<(CycleRow, Vec<MonthRow>)> = r
            .cycles
            .iter()
            .filter(|c| future || c.start <= today)
            .map(|c| {
                let months = r
                    .months
                    .iter()
                    .filter(|m| {
                        let first = NaiveDate::from_ymd_opt(m.year, m.month, 1).unwrap();
                        let in_cycle =
                            first <= c.end && leave_core::calendar::last_of_month(first) >= c.start;
                        in_cycle && (future || first <= today)
                    })
                    .cloned()
                    .collect();
                (c.clone(), months)
            })
            .collect();
        out.reverse();
        out
    });

    view! {
        <section class="page-head">
            <div>
                <h1>"Statement"</h1>
                <p class="sub">"Every day accounted for. Use this to check HR's numbers line by line."</p>
            </div>
            <label class="toggle-label">
                <input type="checkbox" prop:checked=show_future on:change=move |e| show_future.set(event_target_checked(&e))/>
                "Show projection"
            </label>
        </section>

        <div class="grid-2 statement-grid">
            <div class="stack">
                <For
                    each=move || cycles.get().into_iter().enumerate()
                    key=|(_, (c, m))| (c.start, c.end, m.len(), (c.closing * 1000.0).round() as i64, (c.taken * 1000.0).round() as i64)
                    children=move |(i, (c, months))| view! { <CycleBlock cycle=c months=months open=i == 0 index=i/> }
                />
            </div>
            <div class="stack">
                <HrCheck/>
                <Adjustments/>
            </div>
        </div>
    }
}

#[component]
fn CycleBlock(cycle: CycleRow, months: Vec<MonthRow>, open: bool, index: usize) -> impl IntoView {
    let app = use_app();
    let expanded = RwSignal::new(open);
    let today = app.today.get_untracked();
    let current = cycle.start <= today && cycle.end >= today;
    let future = cycle.start > today;
    view! {
        <section class="card cycle reveal" style=format!("--i:{}", index.min(8)) class:expanded=expanded>
            <button class="cycle-head" on:click=move |_| expanded.update(|v| *v = !*v) aria-expanded=move || expanded.get().to_string()>
                <div class="cycle-title">
                    <strong>{format!("{} – {}", fmt::date_med(cycle.start), fmt::date_med(cycle.end))}</strong>
                    {current.then(|| view! { <span class="badge info">"Current · projected to cycle end"</span> })}
                    {future.then(|| view! { <span class="badge">"Projected"</span> })}
                </div>
                <div class="cycle-figs">
                    <span><small>"Opening"</small>{fmt::days(cycle.opening)}</span>
                    <span><small>"Earned"</small>{fmt::days_signed(cycle.accrued)}</span>
                    <span><small>"Taken"</small>{fmt::days(-cycle.taken)}</span>
                    {(cycle.adjusted.abs() > 0.004).then(|| view! { <span><small>"Adjusted"</small>{fmt::days_signed(cycle.adjusted)}</span> })}
                    {(cycle.lost > 0.004).then(|| view! { <span class="lost"><small>"Forfeited"</small>{fmt::days(-cycle.lost)}</span> })}
                    <span class="closing"><small>"Closing"</small>{fmt::days(cycle.closing)}</span>
                </div>
                <span class="chev"><Icon name="chev-down"/></span>
            </button>
            <div class="collapse">
                <div class="collapse-inner">
                    <div class="table-wrap">
                        <table class="ledger">
                            <thead>
                                <tr>
                                    <th>"Month"</th>
                                    <th class="num">"Rate"</th>
                                    <th class="num">"Opening"</th>
                                    <th class="num">"Earned"</th>
                                    <th class="num">"Taken"</th>
                                    <th class="num">"Adj."</th>
                                    <th class="num">"Lost"</th>
                                    <th class="num">"Closing"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {months.into_iter().rev().map(|m| {
                                    let first = NaiveDate::from_ymd_opt(m.year, m.month, 1).unwrap();
                                    let is_now = first.year() == today.year() && first.month() == today.month();
                                    let is_future = first > today;
                                    let dash = |v: f64, neg: bool| if v.abs() < 0.004 { "·".to_string() } else if neg { fmt::days(-v) } else { fmt::days_signed(v) };
                                    view! {
                                        <tr class:now=is_now class:future=is_future>
                                            <td>{format!("{} {}", fmt::month_abbr(m.month), m.year)}</td>
                                            <td class="num muted">{fmt::days(m.rate)}</td>
                                            <td class="num">{fmt::days(m.opening)}</td>
                                            <td class="num pos">{dash(m.accrued, false)}</td>
                                            <td class="num">{dash(m.taken, true)}</td>
                                            <td class="num">{dash(m.adjusted, false)}</td>
                                            <td class="num">{dash(m.lost, true)}</td>
                                            <td class="num strong">{fmt::days(m.closing)}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </section>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HrBasis {
    Today,
    CycleEnd,
}

#[component]
fn HrCheck() -> impl IntoView {
    let app = use_app();
    let basis = RwSignal::new(HrBasis::CycleEnd);
    let date = RwSignal::new(fmt::iso(app.today.get_untracked()));
    let figure = RwSignal::new(String::new());

    let result = Memo::new(move |_| {
        let d = fmt::parse_iso(&date.get())?;
        let r = app.report.get()?;
        let hr = parse_num(&figure.get());
        let (label, mine) = match basis.get() {
            HrBasis::Today => (format!("Balance on {}", fmt::date_med(d)), r.balance_on(d)),
            HrBasis::CycleEnd => {
                let (_, ce) = app.data.with(|x| {
                    leave_core::engine::cycle_bounds(&x.settings, r.start, d.max(r.start))
                });
                (
                    format!(
                        "Left at cycle end ({}), after booked leave",
                        fmt::date_med(ce)
                    ),
                    r.balance_on(ce),
                )
            }
        };
        let pending = if app
            .data
            .with(|x| x.settings.credit_timing == CreditTiming::MonthEnd)
            && basis.get() == HrBasis::Today
        {
            r.pending_on(d)
        } else {
            0.0
        };
        Some((label, mine, pending, hr, d))
    });

    let accept = move |_| {
        let Some((_, mine, _, Some(hr), d)) = result.get_untracked() else {
            return;
        };
        if basis.get_untracked() == HrBasis::CycleEnd {
            // Record the difference as a dated correction.
            let delta = hr - mine;
            app.mutate_with_undo("Correction recorded", |data| {
                let id = data.take_id();
                data.adjustments.push(Adjustment {
                    id,
                    date: d,
                    kind: AdjustmentKind::Delta(delta),
                    note: "Correction to match HR".into(),
                });
            });
        } else {
            app.mutate_with_undo("HR balance recorded", |data| {
                let id = data.take_id();
                data.adjustments.push(Adjustment {
                    id,
                    date: d,
                    kind: AdjustmentKind::SetBalance(hr),
                    note: "Balance confirmed by HR".into(),
                });
            });
        }
        figure.set(String::new());
    };

    view! {
        <section class="card hr reveal" style="--i:1">
            <header class="card-head">
                <h3><Icon name="scale"/>"Check against HR"</h3>
            </header>
            <p class="card-sub">"Type the figure HR gave you to see exactly how it compares."</p>
            <Segmented
                options=vec![(HrBasis::CycleEnd, "Remaining this cycle"), (HrBasis::Today, "Balance on a date")]
                value=basis
                on_change=Callback::new(move |b| basis.set(b))
                label="What HR quoted"
            />
            <div class="form-row">
                <label class="field">
                    <span>{move || if basis.get() == HrBasis::Today { "As at" } else { "Quoted on" }}</span>
                    <input type="date" prop:value=date on:input=move |e| date.set(event_target_value(&e))/>
                </label>
                <label class="field">
                    <span>"HR says (days)"</span>
                    <input type="text" inputmode="decimal" placeholder="e.g. 6" prop:value=figure on:input=move |e| figure.set(event_target_value(&e))/>
                </label>
            </div>
            {move || result.get().map(|(label, mine, pending, hr, _)| {
                let diff = hr.map(|h| h - mine);
                view! {
                    <div class="compare">
                        <div class="cmp">
                            <small>{label}</small>
                            <strong>{fmt::days(mine)}</strong>
                            {(pending > 0.004).then(|| view! { <small class="muted">{format!("+{} accrued, not yet credited", fmt::days(pending))}</small> })}
                        </div>
                        {diff.map(|df| {
                            let ok = df.abs() < 0.005;
                            view! {
                                <div class="cmp-result" class:ok=ok class:off=!ok>
                                    <Icon name=if ok { "check" } else { "alert" }/>
                                    {if ok {
                                        "Matches HR exactly.".to_string()
                                    } else if df < 0.0 {
                                        format!("HR is {} lower than your record.", fmt::plural_days(-df))
                                    } else {
                                        format!("HR is {} higher than your record.", fmt::plural_days(df))
                                    }}
                                </div>
                                {(!ok).then(|| view! {
                                    <p class="hint">"Check public holidays counted as leave, half days, and carried-over days first. If HR is right, record their figure:"</p>
                                    <button class="btn small" on:click=accept>"Use HR's figure"</button>
                                })}
                            }
                        })}
                    </div>
                }
            })}
        </section>
    }
}

#[component]
fn Adjustments() -> impl IntoView {
    let app = use_app();
    let date = RwSignal::new(fmt::iso(app.today.get_untracked()));
    let mode = RwSignal::new(false); // false = add/remove days, true = set balance
    let amount = RwSignal::new(String::new());
    let note = RwSignal::new(String::new());

    let list = Memo::new(move |_| {
        let r = app.report.get();
        app.data.with(|d| {
            let mut v: Vec<(Adjustment, Option<f64>)> = d
                .adjustments
                .iter()
                .map(|a| {
                    let eff = r
                        .as_ref()
                        .and_then(|r| r.adjustments.iter().find(|x| x.id == a.id).map(|x| x.delta));
                    (a.clone(), eff)
                })
                .collect();
            v.sort_by_key(|a| std::cmp::Reverse(a.0.date));
            v
        })
    });

    let add = move |_| {
        let (Some(d), Some(v)) = (
            fmt::parse_iso(&date.get_untracked()),
            parse_num(&amount.get_untracked()),
        ) else {
            return;
        };
        let set = mode.get_untracked();
        let n = note.get_untracked().trim().to_string();
        app.mutate_with_undo("Adjustment added", |data| {
            let id = data.take_id();
            data.adjustments.push(Adjustment {
                id,
                date: d,
                kind: if set {
                    AdjustmentKind::SetBalance(v)
                } else {
                    AdjustmentKind::Delta(v)
                },
                note: n,
            });
        });
        amount.set(String::new());
        note.set(String::new());
    };

    view! {
        <section class="card reveal" style="--i:2">
            <header class="card-head">
                <h3>"Adjustments"</h3>
            </header>
            <p class="card-sub">"Opening balances, HR-confirmed figures, bonus days or corrections."</p>
            <Segmented
                options=vec![(false, "Add / remove days"), (true, "Set balance to")]
                value=mode
                on_change=Callback::new(move |m| mode.set(m))
                label="Adjustment type"
            />
            <div class="form-row">
                <label class="field">
                    <span>"Date"</span>
                    <input type="date" prop:value=date on:input=move |e| date.set(event_target_value(&e))/>
                </label>
                <label class="field">
                    <span>{move || if mode.get() { "Balance (days)" } else { "Days (+/−)" }}</span>
                    <input type="text" inputmode="decimal" placeholder=move || if mode.get() { "e.g. 7.5" } else { "e.g. -1 or 2" } prop:value=amount on:input=move |e| amount.set(event_target_value(&e))/>
                </label>
            </div>
            <label class="field">
                <span>"Note"</span>
                <input type="text" placeholder="e.g. Opening balance from HR" prop:value=note on:input=move |e| note.set(event_target_value(&e))/>
            </label>
            <button class="btn" on:click=add disabled=move || parse_num(&amount.get()).is_none() || fmt::parse_iso(&date.get()).is_none()>
                <Icon name="plus"/>"Add adjustment"
            </button>
            <div class="list compact adj-list">
                <For
                    each=move || list.get()
                    key=|(a, eff)| (a.id, eff.map(|e| (e * 1000.0).round() as i64))
                    children=move |(a, eff)| {
                        let id = a.id;
                        let what = match a.kind {
                            AdjustmentKind::Delta(v) => format!("{} days", fmt::days_signed(v)),
                            AdjustmentKind::SetBalance(v) => format!("Balance set to {}", fmt::days(v)),
                        };
                        view! {
                            <div class="row static">
                                <span class="row-main">
                                    <span class="row-title">{what}</span>
                                    <span class="row-sub">
                                        {fmt::date_long(a.date)}
                                        {(!a.note.is_empty()).then(|| format!(" — {}", a.note))}
                                        {match (a.kind, eff) {
                                            (AdjustmentKind::SetBalance(_), Some(e)) => format!(" · effect {}", fmt::days_signed(e)),
                                            _ => String::new(),
                                        }}
                                    </span>
                                </span>
                                <button class="icon-btn" aria-label="Delete adjustment" on:click=move |_| {
                                    app.mutate_with_undo("Adjustment deleted", |d| d.adjustments.retain(|x| x.id != id));
                                }>
                                    <Icon name="trash"/>
                                </button>
                            </div>
                        }
                    }
                />
            </div>
        </section>
    }
}
