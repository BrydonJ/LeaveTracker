//! Leave list and the add/edit sheet.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{Datelike, Days, NaiveDate};
use leave_core::engine::entry_days;
use leave_core::{compute, LeaveEntry, LeaveKind, WorkCalendar};
use leptos::ev;
use leptos::html;
use leptos::leptos_dom::helpers::{request_animation_frame, set_timeout, window_event_listener};
use leptos::prelude::*;

use crate::fmt;
use crate::ui::{parse_num, Icon, Switch};
use crate::use_app;

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub id: Option<u64>,
    pub kind: LeaveKind,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub half_day: bool,
    pub override_days: Option<f64>,
    pub note: String,
}

impl Draft {
    pub fn new(kind: LeaveKind, start: NaiveDate, end: NaiveDate) -> Self {
        Self {
            id: None,
            kind,
            start,
            end: end.max(start),
            half_day: false,
            override_days: None,
            note: String::new(),
        }
    }

    pub fn from_entry(e: &LeaveEntry) -> Self {
        Self {
            id: Some(e.id),
            kind: e.kind,
            start: e.start,
            end: e.end,
            half_day: e.half_day,
            override_days: e.days_override,
            note: e.note.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Preview {
    days: f64,
    working: usize,
    weekend: usize,
    holidays: Vec<(NaiveDate, String)>,
    balance_after: Option<f64>,
    overlaps: Vec<String>,
}

#[component]
pub fn EntrySheet() -> impl IntoView {
    let app = use_app();
    move || app.sheet.get().map(|d| view! { <SheetBody draft=d/> })
}

#[component]
fn SheetBody(draft: Draft) -> impl IntoView {
    let app = use_app();
    let editing = draft.id;
    let kind = RwSignal::new(draft.kind);
    let start = RwSignal::new(fmt::iso(draft.start));
    let end = RwSignal::new(fmt::iso(draft.end));
    let half = RwSignal::new(draft.half_day);
    let override_on = RwSignal::new(draft.override_days.is_some());
    let override_val = RwSignal::new(draft.override_days.map(fmt::days).unwrap_or_default());
    let note = RwSignal::new(draft.note.clone());
    let closing = RwSignal::new(false);
    let panel = NodeRef::<html::Div>::new();

    let close = move || {
        if closing.get_untracked() {
            return;
        }
        closing.set(true);
        set_timeout(move || app.sheet.set(None), Duration::from_millis(220));
    };

    let dates = move || {
        let s = fmt::parse_iso(&start.get())?;
        let e = fmt::parse_iso(&end.get())?;
        Some((s, e))
    };

    let build_entry = move |id: u64| -> Option<LeaveEntry> {
        let (s, e) = dates()?;
        Some(LeaveEntry {
            id,
            kind: kind.get_untracked(),
            start: s,
            end: e,
            half_day: half.get_untracked(),
            days_override: if override_on.get_untracked() {
                parse_num(&override_val.get_untracked())
            } else {
                None
            },
            note: note.get_untracked().trim().to_string(),
        })
    };

    let preview = Memo::new(move |_| {
        let (s, e) = dates()?;
        if e < s {
            return None;
        }
        let k = kind.get();
        let half_v = half.get();
        let ov = if override_on.get() {
            parse_num(&override_val.get())
        } else {
            None
        };
        app.data.with(|data| {
            let cal = WorkCalendar::new(&data.settings, s.year() - 1, e.year() + 1);
            let entry = LeaveEntry {
                id: editing.unwrap_or(u64::MAX),
                kind: k,
                start: s,
                end: e,
                half_day: half_v,
                days_override: ov,
                note: String::new(),
            };
            let (days, working) = entry_days(&cal, &entry);
            let total = (e - s).num_days() as usize + 1;
            let holidays = cal.skipped_holidays(s, e);
            let weekend = total - working.len() - holidays.len();
            let balance_after = if k.deducts() {
                let mut d2 = data.clone();
                d2.entries.retain(|x| Some(x.id) != editing);
                d2.entries.push(entry);
                compute(&d2, app.today.get_untracked(), Some(e)).map(|r| r.balance_on(e))
            } else {
                None
            };
            let overlaps = data
                .entries
                .iter()
                .filter(|x| Some(x.id) != editing && x.start <= e && x.end >= s)
                .map(|x| format!("{} leave, {}", x.kind.label(), fmt::range(x.start, x.end)))
                .collect();
            Some(Preview {
                days,
                working: working.len(),
                weekend,
                holidays,
                balance_after,
                overlaps,
            })
        })
    });

    let error = move || -> Option<&'static str> {
        let Some((s, e)) = dates() else {
            return Some("Pick a start and end date.");
        };
        if e < s {
            return Some("The end date is before the start date.");
        }
        if override_on.get() && parse_num(&override_val.get()).is_none() {
            return Some("Enter the number of days to charge.");
        }
        None
    };

    let save = move |again: bool| {
        if error().is_some() {
            return;
        }
        let before = app.data.get_untracked();
        let mut saved_end = None;
        app.data.update(|d| {
            let id = editing.unwrap_or_else(|| d.take_id());
            if let Some(entry) = build_entry(id) {
                saved_end = Some(entry.end);
                if let Some(pos) = d.entries.iter().position(|x| x.id == id) {
                    d.entries[pos] = entry;
                } else {
                    d.entries.push(entry);
                }
            }
        });
        app.notify(
            if editing.is_some() {
                "Leave updated"
            } else {
                "Leave added"
            },
            Some(before),
        );
        if again {
            // Back-capture flow: keep the type, move to the next day.
            let next = saved_end
                .map(|e| e + Days::new(1))
                .unwrap_or(app.today.get_untracked());
            start.set(fmt::iso(next));
            end.set(fmt::iso(next));
            half.set(false);
            override_on.set(false);
            override_val.set(String::new());
            note.set(String::new());
        } else {
            close();
        }
    };

    let delete = move |_| {
        if let Some(id) = editing {
            app.mutate_with_undo("Leave deleted", |d| d.entries.retain(|x| x.id != id));
            close();
        }
    };

    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" {
            close();
        }
    });
    on_cleanup(move || esc.remove());
    request_animation_frame(move || {
        if let Some(p) = panel.try_get_untracked().flatten() {
            let _ = p.focus();
        }
    });

    let single_day = move || preview.get().map(|p| p.working == 1).unwrap_or(false);

    view! {
        <div class="sheet-root" class:closing=closing>
            <div class="sheet-backdrop" on:click=move |_| close()></div>
            <div
                class="sheet"
                role="dialog"
                aria-modal="true"
                aria-labelledby="sheet-title"
                tabindex="-1"
                node_ref=panel
            >
                <div class="sheet-grip" aria-hidden="true"></div>
                <header class="sheet-head">
                    <h2 id="sheet-title">{if editing.is_some() { "Edit leave" } else { "Add leave" }}</h2>
                    <button class="icon-btn" on:click=move |_| close() aria-label="Close">
                        <Icon name="x"/>
                    </button>
                </header>
                <form
                    class="sheet-body"
                    on:submit=move |e: ev::SubmitEvent| {
                        e.prevent_default();
                        save(false);
                    }
                >
                    <div class="kind-grid" role="radiogroup" aria-label="Leave type">
                        {LeaveKind::ALL
                            .into_iter()
                            .map(|k| {
                                view! {
                                    <button
                                        type="button"
                                        role="radio"
                                        class=format!("kind-chip k-{}", k.slug())
                                        class:active=move || kind.get() == k
                                        aria-checked=move || (kind.get() == k).to_string()
                                        on:click=move |_| kind.set(k)
                                    >
                                        <span class="dot"></span>
                                        {k.label()}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>

                    <div class="date-row">
                        <label class="field">
                            <span>"From"</span>
                            <input
                                type="date"
                                required
                                prop:value=start
                                on:input=move |e| {
                                    let v = event_target_value(&e);
                                    if let (Some(s), Some(en)) = (fmt::parse_iso(&v), fmt::parse_iso(&end.get_untracked())) {
                                        if en < s {
                                            end.set(v.clone());
                                        }
                                    }
                                    start.set(v);
                                }
                            />
                        </label>
                        <span class="date-arrow" aria-hidden="true">
                            <Icon name="chev-right"/>
                        </span>
                        <label class="field">
                            <span>"To"</span>
                            <input
                                type="date"
                                required
                                prop:value=end
                                prop:min=start
                                on:input=move |e| end.set(event_target_value(&e))
                            />
                        </label>
                    </div>

                    <div class="preview" class:muted=move || !kind.get().deducts()>
                        {move || match preview.get() {
                            None => view! { <div class="preview-empty">"Choose valid dates to see the count."</div> }.into_any(),
                            Some(p) => {
                                let skipped = p.holidays.clone();
                                view! {
                                    <div class="preview-main">
                                        <div class="preview-num">
                                            <span class="big">{fmt::days(p.days)}</span>
                                            <span class="unit">{if fmt::days(p.days) == "1" { "day" } else { "days" }}</span>
                                        </div>
                                        <div class="preview-meta">
                                            <span>
                                                {if kind.get().deducts() { "from annual leave" } else { "recorded, not deducted" }}
                                            </span>
                                            {p.balance_after.map(|b| view! {
                                                <span class="after" class:neg=b < -0.004>
                                                    {format!("{} left after", fmt::days(b))}
                                                </span>
                                            })}
                                        </div>
                                    </div>
                                    <ul class="preview-notes">
                                        {(p.weekend > 0).then(|| view! {
                                            <li>{format!("Skips {} weekend day{}", p.weekend, if p.weekend == 1 { "" } else { "s" })}</li>
                                        })}
                                        {skipped.into_iter().map(|(d, n)| view! {
                                            <li class="hol">{format!("Skips {} · {}", fmt::date_short(d), n)}</li>
                                        }).collect_view()}
                                        {p.overlaps.iter().map(|o| view! {
                                            <li class="warn"><Icon name="alert"/>{format!("Overlaps {o}")}</li>
                                        }).collect_view()}
                                        {p.balance_after.filter(|b| *b < -0.004).map(|_| view! {
                                            <li class="warn"><Icon name="alert"/>"This takes your balance negative."</li>
                                        })}
                                    </ul>
                                }.into_any()
                            }
                        }}
                    </div>

                    <div class="opt-row" class:disabled=move || !single_day() || override_on.get()>
                        <div>
                            <div class="opt-title">"Half day"</div>
                            <div class="opt-sub">"Counts a single working day as 0.5"</div>
                        </div>
                        <Switch
                            checked=Signal::derive(move || half.get() && single_day())
                            on_change=Callback::new(move |v| half.set(v))
                            label="Half day"
                        />
                    </div>
                    <div class="opt-row">
                        <div>
                            <div class="opt-title">"Override day count"</div>
                            <div class="opt-sub">"Charge a specific number of days instead"</div>
                        </div>
                        <Switch
                            checked=override_on
                            on_change=Callback::new(move |v: bool| {
                                if v && override_val.get_untracked().is_empty() {
                                    override_val.set(preview.get_untracked().map(|p| fmt::days(p.days)).unwrap_or_default());
                                }
                                override_on.set(v)
                            })
                            label="Override day count"
                        />
                    </div>
                    <Show when=move || override_on.get()>
                        <label class="field reveal">
                            <span>"Days to charge"</span>
                            <input
                                type="text"
                                inputmode="decimal"
                                prop:value=override_val
                                on:input=move |e| override_val.set(event_target_value(&e))
                            />
                        </label>
                    </Show>
                    <label class="field">
                        <span>"Note"</span>
                        <input
                            type="text"
                            placeholder="Optional — e.g. Cape Town trip"
                            prop:value=note
                            on:input=move |e| note.set(event_target_value(&e))
                        />
                    </label>

                    {move || error().map(|e| view! { <p class="form-error">{e}</p> })}

                    <div class="sheet-actions">
                        {editing.map(|_| view! {
                            <button type="button" class="btn danger ghost" on:click=delete>
                                <Icon name="trash"/>"Delete"
                            </button>
                        })}
                        <span class="spacer"></span>
                        {editing.is_none().then(|| view! {
                            <button type="button" class="btn ghost" on:click=move |_| save(true) disabled=move || error().is_some()>
                                "Save & add another"
                            </button>
                        })}
                        <button type="submit" class="btn primary" disabled=move || error().is_some()>
                            <Icon name="check"/>
                            {if editing.is_some() { "Save" } else { "Add leave" }}
                        </button>
                    </div>
                </form>
            </div>
        </div>
    }
}

#[component]
pub fn LeaveList() -> impl IntoView {
    let app = use_app();
    let filter = RwSignal::new(None::<LeaveKind>);
    let query = RwSignal::new(String::new());

    let groups = Memo::new(move |_| {
        let f = filter.get();
        let q = query.get().to_lowercase();
        let report = app.report.get();
        app.data.with(|d| {
            let mut by_year: BTreeMap<i32, Vec<(LeaveEntry, f64)>> = BTreeMap::new();
            for e in &d.entries {
                if f.is_some_and(|k| k != e.kind) {
                    continue;
                }
                if !q.is_empty()
                    && !e.note.to_lowercase().contains(&q)
                    && !e.kind.label().to_lowercase().contains(&q)
                {
                    continue;
                }
                let days = report
                    .as_ref()
                    .and_then(|r| r.entry_days.get(&e.id).copied())
                    .unwrap_or(0.0);
                by_year
                    .entry(e.start.year())
                    .or_default()
                    .push((e.clone(), days));
            }
            by_year
                .into_iter()
                .rev()
                .map(|(y, mut v)| {
                    v.sort_by(|a, b| b.0.start.cmp(&a.0.start).then(b.0.id.cmp(&a.0.id)));
                    (y, v)
                })
                .collect::<Vec<_>>()
        })
    });

    let present = Memo::new(move |_| {
        app.data.with(|d| {
            LeaveKind::ALL
                .into_iter()
                .filter(|k| d.entries.iter().any(|e| e.kind == *k))
                .collect::<Vec<_>>()
        })
    });

    let has_entries = move || app.data.with(|d| !d.entries.is_empty());

    view! {
        <section class="page-head">
            <div>
                <h1>"Leave"</h1>
                <p class="sub">"Everything you've taken and booked. Tap an entry to edit it."</p>
            </div>
        </section>

        <Show
            when=has_entries
            fallback=move || view! {
                <div class="card empty">
                    <div class="empty-art"><Icon name="calendar"/></div>
                    <h3>"No leave captured yet"</h3>
                    <p>"Back-capture your past leave so the balance matches reality. Use “Save & add another” to move fast."</p>
                    <button class="btn primary" on:click=move |_| app.open_new(None, None)>
                        <Icon name="plus"/>"Add your first leave"
                    </button>
                </div>
            }
        >
            <div class="toolbar">
                <div class="chips" role="radiogroup" aria-label="Filter by type">
                    <button class="chip" class:active=move || filter.get().is_none() on:click=move |_| filter.set(None)>
                        "All"
                    </button>
                    <For
                        each=move || present.get()
                        key=|k| *k
                        children=move |k| view! {
                            <button
                                class=format!("chip k-{}", k.slug())
                                class:active=move || filter.get() == Some(k)
                                on:click=move |_| filter.set(if filter.get_untracked() == Some(k) { None } else { Some(k) })
                            >
                                <span class="dot"></span>
                                {k.label()}
                            </button>
                        }
                    />
                </div>
                <input
                    class="search"
                    type="search"
                    placeholder="Search notes"
                    prop:value=query
                    on:input=move |e| query.set(event_target_value(&e))
                />
            </div>

            {move || {
                let today = app.today.get();
                let groups = groups.get();
                if groups.is_empty() {
                    return view! { <p class="muted center pad">"Nothing matches that filter."</p> }.into_any();
                }
                groups
                    .into_iter()
                    .map(|(year, items)| {
                        let annual: f64 = items.iter().filter(|(e, _)| e.kind.deducts()).map(|x| x.1).sum();
                        let other: f64 = items.iter().filter(|(e, _)| !e.kind.deducts()).map(|x| x.1).sum();
                        view! {
                            <section class="year-group">
                                <header class="year-head">
                                    <h2>{year}</h2>
                                    <span class="year-sum">
                                        {format!("{} annual", fmt::plural_days(annual))}
                                        {(other > 0.0).then(|| format!(" · {} other", fmt::plural_days(other)))}
                                    </span>
                                </header>
                                <div class="card list">
                                    {items
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, (e, days))| view! { <EntryRow entry=e days=days today=today index=i/> })
                                        .collect_view()}
                                </div>
                            </section>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </Show>
    }
}

#[component]
pub fn EntryRow(entry: LeaveEntry, days: f64, today: NaiveDate, index: usize) -> impl IntoView {
    let app = use_app();
    let upcoming = entry.start > today;
    let ongoing = entry.start <= today && entry.end >= today;
    let draft = Draft::from_entry(&entry);
    let sub = {
        let mut parts = vec![entry.kind.label().to_string()];
        if entry.half_day {
            parts.push("half day".into());
        }
        if entry.days_override.is_some() {
            parts.push("manual count".into());
        }
        parts.join(" · ")
    };
    view! {
        <button
            class=format!("row k-{}", entry.kind.slug())
            style=format!("--i:{}", index.min(14))
            on:click=move |_| app.sheet.set(Some(draft.clone()))
        >
            <span class="row-bar" aria-hidden="true"></span>
            <span class="row-main">
                <span class="row-title">{fmt::range(entry.start, entry.end)}</span>
                <span class="row-sub">
                    {sub}
                    {(!entry.note.is_empty()).then(|| view! { <span class="row-note">{format!(" — {}", entry.note)}</span> })}
                </span>
            </span>
            <span class="row-side">
                <span class="row-days">{fmt::days(days)}<small>{if fmt::days(days) == "1" { " day" } else { " days" }}</small></span>
                {upcoming.then(|| view! { <span class="badge info">{fmt::relative(entry.start, today)}</span> })}
                {ongoing.then(|| view! { <span class="badge good">"On leave"</span> })}
                {(entry.kind.deducts() && days.abs() < 0.004 && entry.days_override.is_none()).then(|| view! {
                    <span class="badge warn" title="Every day in this range is a weekend or public holiday">"No working days"</span>
                })}
            </span>
        </button>
    }
}
