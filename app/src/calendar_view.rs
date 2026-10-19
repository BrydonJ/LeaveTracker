//! Year-at-a-glance calendar. Drag across days (or tap one) to book leave.

use std::collections::HashMap;

use chrono::{Datelike, Days, NaiveDate};
use leave_core::{LeaveEntry, LeaveKind};
use leptos::ev;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;

use crate::fmt;
use crate::leave::Draft;
use crate::ui::Icon;
use crate::use_app;

#[derive(Clone, Debug, PartialEq)]
struct DayInfo {
    kind: Option<LeaveKind>,
    entry: Option<u64>,
    working: bool,
    holiday: Option<String>,
}

#[component]
pub fn CalendarView() -> impl IntoView {
    let app = use_app();
    let year = RwSignal::new(app.today.get_untracked().year());
    let anchor = RwSignal::new(None::<NaiveDate>);
    let cursor = RwSignal::new(None::<NaiveDate>);
    let dragging = StoredValue::new(false);

    let info = Memo::new(move |_| {
        let y = year.get();
        app.data.with(|d| {
            let cal = leave_core::WorkCalendar::new(&d.settings, y - 1, y + 1);
            let first = NaiveDate::from_ymd_opt(y, 1, 1).unwrap();
            let last = NaiveDate::from_ymd_opt(y, 12, 31).unwrap();
            let mut covering: HashMap<NaiveDate, &LeaveEntry> = HashMap::new();
            let mut sorted: Vec<&LeaveEntry> = d
                .entries
                .iter()
                .filter(|e| e.end >= first && e.start <= last)
                .collect();
            // Annual leave wins the colour when entries overlap.
            sorted.sort_by_key(|e| std::cmp::Reverse(e.kind.deducts()));
            for e in sorted {
                for day in e
                    .start
                    .max(first)
                    .iter_days()
                    .take_while(|x| *x <= e.end.min(last))
                {
                    covering.entry(day).or_insert(e);
                }
            }
            first
                .iter_days()
                .take_while(|x| *x <= last)
                .map(|day| {
                    let e = covering.get(&day);
                    (
                        day,
                        DayInfo {
                            kind: e.map(|e| e.kind),
                            entry: e.map(|e| e.id),
                            working: cal.is_working_day(day),
                            holiday: cal.holiday(day).map(str::to_string),
                        },
                    )
                })
                .collect::<HashMap<_, _>>()
        })
    });

    let year_totals = Memo::new(move |_| {
        let y = year.get();
        let r = app.report.get();
        app.data.with(|d| {
            let Some(r) = r else { return (0.0, 0.0) };
            let a = NaiveDate::from_ymd_opt(y, 1, 1).unwrap();
            let b = NaiveDate::from_ymd_opt(y, 12, 31).unwrap();
            let annual = r.taken_between(d, a, b);
            let holidays = info.with(|m| m.values().filter(|i| i.holiday.is_some()).count() as f64);
            (annual, holidays)
        })
    });

    let in_selection = move |d: NaiveDate| match (anchor.get(), cursor.get()) {
        (Some(a), Some(c)) => d >= a.min(c) && d <= a.max(c),
        _ => false,
    };

    // Opens the day's entry for editing, or a new entry on that day.
    let open_day = move |d: NaiveDate| {
        let existing = info.with_untracked(|m| m.get(&d).and_then(|i| i.entry));
        if let Some(id) = existing {
            let draft = app
                .data
                .with_untracked(|x| x.entries.iter().find(|e| e.id == id).map(Draft::from_entry));
            if let Some(draft) = draft {
                app.sheet.set(Some(draft));
                return;
            }
        }
        app.open_new(Some(d), Some(d));
    };

    // Mouse drags select a range; taps and single clicks go through `click`.
    let suppress_click = StoredValue::new(false);
    let finish = move || {
        if !dragging.get_value() {
            return;
        }
        dragging.set_value(false);
        let (a, c) = (anchor.get_untracked(), cursor.get_untracked());
        anchor.set(None);
        cursor.set(None);
        if let (Some(a), Some(c)) = (a, c) {
            if a != c {
                suppress_click.set_value(true);
                app.open_new(Some(a.min(c)), Some(a.max(c)));
            }
        }
    };
    let up = window_event_listener(ev::pointerup, move |_| finish());
    let cancel = window_event_listener(ev::pointercancel, move |_| {
        dragging.set_value(false);
        anchor.set(None);
        cursor.set(None);
    });
    on_cleanup(move || {
        up.remove();
        cancel.remove();
    });

    let today = app.today;

    view! {
        <section class="page-head">
            <div>
                <h1>"Calendar"</h1>
                <p class="sub">"Drag across days to book leave. Tap a coloured day to edit it."</p>
            </div>
            <div class="year-switch">
                <button class="icon-btn" aria-label="Previous year" on:click=move |_| year.update(|y| *y -= 1)>
                    <Icon name="chev-left"/>
                </button>
                <span class="year-label">{year}</span>
                <button class="icon-btn" aria-label="Next year" on:click=move |_| year.update(|y| *y += 1)>
                    <Icon name="chev-right"/>
                </button>
            </div>
        </section>

        <div class="legend">
            <span class="lg k-annual"><span class="sw"></span>"Annual"</span>
            <span class="lg k-sick"><span class="sw"></span>"Sick"</span>
            <span class="lg k-family"><span class="sw"></span>"Family"</span>
            <span class="lg k-other"><span class="sw"></span>"Study / unpaid / other"</span>
            <span class="lg hol"><span class="sw"></span>"Public holiday"</span>
            <span class="lg-sum">
                {move || {
                    let (a, h) = year_totals.get();
                    format!("{} annual leave · {} holidays", fmt::plural_days(a), h)
                }}
            </span>
        </div>

        {move || {
            let y = year.get();
            let map = info.get();
            view! {
                <div class="months">
                    {(1..=12u32).map(|m| {
                        let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
                        let lead = first.weekday().num_days_from_monday() as usize;
                        let n = leave_core::calendar::days_in_month(first);
                        view! {
                            <div class="month card" style=format!("--i:{}", m - 1)>
                                <h4>{fmt::month_name(m)}</h4>
                                <div class="dow">
                                    {["M", "T", "W", "T", "F", "S", "S"].iter().map(|d| view! { <span>{*d}</span> }).collect_view()}
                                </div>
                                <div class="days">
                                    {(0..lead).map(|_| view! { <span class="day blank"></span> }).collect_view()}
                                    {(0..n).map(|i| {
                                        let d = first + Days::new(i as u64);
                                        let di = map.get(&d).cloned();
                                        let di = di.unwrap_or(DayInfo { kind: None, entry: None, working: true, holiday: None });
                                        let mut cls = String::from("day");
                                        if let Some(k) = di.kind {
                                            cls.push_str(if di.working { " leave k-" } else { " leave-off k-" });
                                            cls.push_str(match k {
                                                LeaveKind::Annual => "annual",
                                                LeaveKind::Sick => "sick",
                                                LeaveKind::Family => "family",
                                                _ => "other",
                                            });
                                        }
                                        if !di.working { cls.push_str(" off"); }
                                        if di.holiday.is_some() { cls.push_str(" hol"); }
                                        let title = {
                                            let mut t = fmt::date_long(d);
                                            if let Some(h) = &di.holiday { t.push_str(&format!(" · {h}")); }
                                            if let Some(k) = di.kind { t.push_str(&format!(" · {} leave", k.label())); }
                                            t
                                        };
                                        view! {
                                            <span
                                                class=cls
                                                class:today=move || today.get() == d
                                                class:sel=move || in_selection(d)
                                                title=title
                                                on:pointerdown=move |e: ev::PointerEvent| {
                                                    if e.button() != 0 || e.pointer_type() != "mouse" { return; }
                                                    e.prevent_default();
                                                    suppress_click.set_value(false);
                                                    dragging.set_value(true);
                                                    anchor.set(Some(d));
                                                    cursor.set(Some(d));
                                                }
                                                on:pointerenter=move |_| {
                                                    if dragging.get_value() { cursor.set(Some(d)); }
                                                }
                                                on:click=move |_| {
                                                    if suppress_click.get_value() {
                                                        suppress_click.set_value(false);
                                                        return;
                                                    }
                                                    open_day(d);
                                                }
                                            >
                                                {d.day()}
                                            </span>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            }
        }}
    }
}
