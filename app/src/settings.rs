//! Settings: employment, entitlement rules, calendar and data management.

use leave_core::engine::years_of_service;
use leave_core::{CreditTiming, CycleBasis, Data, Holiday, RateBasis, Settings};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::fmt;
use crate::ui::{parse_num, Icon, Segmented, Switch};
use crate::{store, use_app, AppState};

fn update(app: AppState, f: impl FnOnce(&mut Settings)) {
    app.data.update(|d| f(&mut d.settings));
}

pub fn export_json(app: AppState) {
    let json = app.data.with_untracked(Data::to_json);
    let name = format!(
        "leave-tracker-backup-{}.json",
        fmt::iso(app.today.get_untracked())
    );
    store::download(&name, "application/json", &json);
    app.last_backup.set(Some(store::mark_backup()));
    app.notify("Backup downloaded", None);
}

fn export_csv(app: AppState) {
    let csv = app.data.with_untracked(|d| {
        let r = app.report.get_untracked();
        let mut entries: Vec<_> = d.entries.iter().collect();
        entries.sort_by_key(|e| e.start);
        let mut out = String::from("type,start,end,days,half_day,note\n");
        for e in entries {
            let days = r
                .as_ref()
                .and_then(|r| r.entry_days.get(&e.id).copied())
                .unwrap_or(0.0);
            out.push_str(&format!(
                "{},{},{},{},{},\"{}\"\n",
                e.kind.label(),
                fmt::iso(e.start),
                fmt::iso(e.end),
                fmt::days(days).replace('−', "-"),
                e.half_day,
                e.note.replace('"', "\"\"")
            ));
        }
        out
    });
    store::download(
        &format!("leave-{}.csv", fmt::iso(app.today.get_untracked())),
        "text/csv",
        &csv,
    );
}

#[component]
fn NumField(
    label: &'static str,
    #[prop(into)] value: Signal<f64>,
    #[prop(into)] on_change: Callback<f64>,
    #[prop(optional)] suffix: &'static str,
    #[prop(default = 0.0)] min: f64,
) -> impl IntoView {
    view! {
        <label class="field">
            <span>{label}</span>
            <div class="input-suffix">
                <input
                    type="text"
                    inputmode="decimal"
                    prop:value=move || fmt::days(value.get())
                    on:change=move |e| {
                        let raw = event_target_value(&e);
                        match parse_num(&raw).filter(|v| *v >= min) {
                            Some(v) => on_change.run(v),
                            None => {
                                if let Some(t) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                    t.set_value(&fmt::days(value.get_untracked()));
                                }
                            }
                        }
                    }
                />
                {(!suffix.is_empty()).then(|| view! { <em>{suffix}</em> })}
            </div>
        </label>
    }
}

#[component]
pub fn SettingsView() -> impl IntoView {
    let app = use_app();
    let s = move || app.data.with(|d| d.settings.clone());
    let s = Memo::new(move |_| s());

    let preview = move || {
        let st = s.get();
        let now_years = st
            .start_date
            .map(|d| years_of_service(d, app.today.get()))
            .unwrap_or(0);
        (0..6u32)
            .map(|y| {
                let mut e = st.base_days + st.increment_per_year * y as f64;
                if let Some(m) = st.max_entitlement {
                    e = e.min(m);
                }
                (y, e, y == now_years)
            })
            .collect::<Vec<_>>()
    };

    let new_hol_date = RwSignal::new(String::new());
    let new_hol_name = RwSignal::new(String::new());
    let file_input = NodeRef::<leptos::html::Input>::new();

    let on_import = move |e: leptos::ev::Event| {
        let Some(input) = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(file) = input.files().and_then(|f| f.get(0)) else {
            return;
        };
        input.set_value("");
        spawn_local(async move {
            let text = match JsFuture::from(file.text()).await {
                Ok(t) => t.as_string().unwrap_or_default(),
                Err(_) => {
                    app.notify("Couldn't read that file", None);
                    return;
                }
            };
            match Data::from_json(&text) {
                Ok(d) => {
                    let n = d.entries.len();
                    if store::confirm(&format!(
                        "Replace everything in this browser with the backup ({n} leave entries)?"
                    )) {
                        let before = app.data.get_untracked();
                        app.data.set(d);
                        app.notify("Backup restored", Some(before));
                    }
                }
                Err(err) => app.notify(format!("Not a valid backup: {err}"), None),
            }
        });
    };

    view! {
        <section class="page-head">
            <div>
                <h1>"Settings"</h1>
                <p class="sub">"Match these to your contract and the balance follows automatically."</p>
            </div>
        </section>

        <div class="settings-grid">
            <section class="card reveal" style="--i:0">
                <header class="card-head"><h3>"Employment"</h3></header>
                <label class="field">
                    <span>"Date you joined"</span>
                    <input
                        type="date"
                        prop:value=move || s.get().start_date.map(fmt::iso).unwrap_or_default()
                        on:change=move |e| {
                            if let Some(d) = fmt::parse_iso(&event_target_value(&e)) {
                                update(app, |s| s.start_date = Some(d));
                            }
                        }
                    />
                </label>
                {move || s.get().start_date.map(|d| {
                    let today = app.today.get();
                    let y = years_of_service(d, today);
                    view! { <p class="hint">{format!("{} completed year{} of service · joined {}", y, if y == 1 { "" } else { "s" }, fmt::relative(d, today))}</p> }
                })}
            </section>

            <section class="card reveal" style="--i:1">
                <header class="card-head"><h3>"Entitlement"</h3></header>
                <div class="form-row">
                    <NumField label="Base days per year" value=Signal::derive(move || s.get().base_days)
                        on_change=Callback::new(move |v| update(app, |s| s.base_days = v)) suffix="days"/>
                    <NumField label="Extra per year worked" value=Signal::derive(move || s.get().increment_per_year)
                        on_change=Callback::new(move |v| update(app, |s| s.increment_per_year = v)) suffix="days"/>
                </div>
                <div class="opt-row">
                    <div>
                        <div class="opt-title">"Cap the entitlement"</div>
                        <div class="opt-sub">"Stop increasing past a maximum"</div>
                    </div>
                    <Switch
                        checked=Signal::derive(move || s.get().max_entitlement.is_some())
                        on_change=Callback::new(move |on: bool| update(app, |s| {
                            s.max_entitlement = on.then_some(s.base_days + s.increment_per_year * 10.0)
                        }))
                        label="Cap the entitlement"
                    />
                </div>
                <Show when=move || s.get().max_entitlement.is_some()>
                    <div class="reveal">
                        <NumField label="Maximum days per year" value=Signal::derive(move || s.get().max_entitlement.unwrap_or(0.0))
                            on_change=Callback::new(move |v| update(app, |s| s.max_entitlement = Some(v))) suffix="days"/>
                    </div>
                </Show>
                <div class="ladder">
                    {move || preview().into_iter().map(|(y, e, now)| view! {
                        <div class="rung" class:now=now>
                            <small>{format!("Year {}", y + 1)}</small>
                            <strong>{fmt::days(e)}</strong>
                        </div>
                    }).collect_view()}
                </div>
            </section>

            <section class="card reveal" style="--i:2">
                <header class="card-head"><h3>"Accrual"</h3></header>
                <div class="setting">
                    <div class="opt-title">"When leave is credited"</div>
                    <Segmented
                        options=vec![(CreditTiming::MonthEnd, "Monthly"), (CreditTiming::Daily, "Daily"), (CreditTiming::Upfront, "Upfront")]
                        value=Signal::derive(move || s.get().credit_timing)
                        on_change=Callback::new(move |v| update(app, |s| s.credit_timing = v))
                        label="When leave is credited"
                    />
                    <p class="hint">{move || match s.get().credit_timing {
                        CreditTiming::MonthEnd => "1/12 of the annual entitlement is credited on the last day of each month. A partial first month is pro-rated by calendar days.",
                        CreditTiming::Daily => "The same monthly amount, spread over each day of the month, so the balance ticks up daily.",
                        CreditTiming::Upfront => "The whole year's allowance is credited on the first day of each leave cycle (pro-rated for your first, partial cycle).",
                    }}</p>
                </div>
                <div class="setting">
                    <div class="opt-title">"Leave cycle"</div>
                    <Segmented
                        options=vec![(CycleBasis::Anniversary, "Work anniversary"), (CycleBasis::Calendar, "Calendar year")]
                        value=Signal::derive(move || s.get().cycle)
                        on_change=Callback::new(move |v| update(app, |s| s.cycle = v))
                        label="Leave cycle"
                    />
                </div>
                <div class="setting">
                    <div class="opt-title">"Yearly increase applies"</div>
                    <Segmented
                        options=vec![(RateBasis::Anniversary, "On anniversary"), (RateBasis::CycleStart, "From next cycle")]
                        value=Signal::derive(move || s.get().rate_basis)
                        on_change=Callback::new(move |v| update(app, |s| s.rate_basis = v))
                        label="Yearly increase applies"
                    />
                    <p class="hint">{move || match s.get().rate_basis {
                        RateBasis::Anniversary => "The extra day starts accruing on your work anniversary.",
                        RateBasis::CycleStart => "Each cycle uses the years you had completed on its first day — e.g. HR's “17 days allowance for 2026”.",
                    }}</p>
                </div>
            </section>

            <section class="card reveal" style="--i:3">
                <header class="card-head"><h3>"Carry-over"</h3></header>
                <div class="opt-row">
                    <div>
                        <div class="opt-title">"Limit carry-over"</div>
                        <div class="opt-sub">"Days above the cap are forfeited at cycle end"</div>
                    </div>
                    <Switch
                        checked=Signal::derive(move || s.get().carry_cap.is_some())
                        on_change=Callback::new(move |on: bool| update(app, |s| s.carry_cap = on.then_some(5.0)))
                        label="Limit carry-over"
                    />
                </div>
                <Show when=move || s.get().carry_cap.is_some()>
                    <div class="reveal">
                        <NumField label="Maximum carried into a new cycle" value=Signal::derive(move || s.get().carry_cap.unwrap_or(0.0))
                            on_change=Callback::new(move |v| update(app, |s| s.carry_cap = Some(v))) suffix="days"/>
                    </div>
                </Show>
                <div class="opt-row">
                    <div>
                        <div class="opt-title">"Carried days expire"</div>
                        <div class="opt-sub">"Must be used within a number of months"</div>
                    </div>
                    <Switch
                        checked=Signal::derive(move || s.get().carry_expiry_months.is_some())
                        on_change=Callback::new(move |on: bool| update(app, |s| s.carry_expiry_months = on.then_some(6)))
                        label="Carried days expire"
                    />
                </div>
                <Show when=move || s.get().carry_expiry_months.is_some()>
                    <div class="reveal">
                        <NumField label="Use within" value=Signal::derive(move || s.get().carry_expiry_months.unwrap_or(0) as f64)
                            on_change=Callback::new(move |v: f64| update(app, |s| s.carry_expiry_months = Some(v.round().max(1.0) as u32))) suffix="months" min=1.0/>
                    </div>
                </Show>
                <p class="hint">"The BCEA allows employers to require annual leave to be taken within 6 months of the cycle ending — check your contract or policy."</p>
            </section>

            <section class="card reveal" style="--i:4">
                <header class="card-head"><h3>"Working days & holidays"</h3></header>
                <div class="setting">
                    <div class="opt-title">"Working days"</div>
                    <div class="weekdays">
                        {["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].into_iter().enumerate().map(|(i, d)| view! {
                            <button
                                type="button"
                                class="chip"
                                class:active=move || s.get().work_days[i]
                                aria-pressed=move || s.get().work_days[i].to_string()
                                on:click=move |_| update(app, |s| s.work_days[i] = !s.work_days[i])
                            >{d}</button>
                        }).collect_view()}
                    </div>
                </div>
                <div class="opt-row">
                    <div>
                        <div class="opt-title">"South African public holidays"</div>
                        <div class="opt-sub">"Incl. Sunday → Monday rule and declared special days"</div>
                    </div>
                    <Switch
                        checked=Signal::derive(move || s.get().sa_holidays)
                        on_change=Callback::new(move |v| update(app, |s| s.sa_holidays = v))
                        label="South African public holidays"
                    />
                </div>
                <div class="setting">
                    <div class="opt-title">"Extra days off"</div>
                    <p class="hint">"Company shutdown days or new one-off holidays that shouldn't cost leave."</p>
                    <div class="form-row">
                        <label class="field">
                            <span>"Date"</span>
                            <input type="date" prop:value=new_hol_date on:input=move |e| new_hol_date.set(event_target_value(&e))/>
                        </label>
                        <label class="field">
                            <span>"Name"</span>
                            <input type="text" placeholder="e.g. Office closed" prop:value=new_hol_name on:input=move |e| new_hol_name.set(event_target_value(&e))/>
                        </label>
                    </div>
                    <button class="btn small" disabled=move || fmt::parse_iso(&new_hol_date.get()).is_none() on:click=move |_| {
                        if let Some(d) = fmt::parse_iso(&new_hol_date.get_untracked()) {
                            let name = new_hol_name.get_untracked().trim().to_string();
                            update(app, |s| {
                                s.custom_holidays.retain(|h| h.date != d);
                                s.custom_holidays.push(Holiday { date: d, name: if name.is_empty() { "Day off".into() } else { name } });
                                s.custom_holidays.sort_by_key(|h| h.date);
                            });
                            new_hol_date.set(String::new());
                            new_hol_name.set(String::new());
                        }
                    }><Icon name="plus"/>"Add day off"</button>
                    <div class="list compact">
                        {move || s.get().custom_holidays.into_iter().map(|h| {
                            let d = h.date;
                            view! {
                                <div class="row static">
                                    <span class="row-main">
                                        <span class="row-title">{h.name}</span>
                                        <span class="row-sub">{fmt::date_long(d)}</span>
                                    </span>
                                    <button class="icon-btn" aria-label="Remove" on:click=move |_| update(app, |s| s.custom_holidays.retain(|x| x.date != d))>
                                        <Icon name="trash"/>
                                    </button>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </div>
            </section>

            <section class="card reveal" style="--i:5">
                <header class="card-head"><h3><Icon name="shield"/>"Your data"</h3></header>
                <p class="card-sub">
                    "Everything stays on this device — nothing is sent anywhere. Export a backup regularly, and import it on another device to move your record."
                </p>
                <p class="hint">
                    {move || match app.last_backup.get() {
                        Some(t) => format!("Last backup {}", fmt::ago_ms(t, store::now_ms())),
                        None => "Never backed up".to_string(),
                    }}
                </p>
                <div class="btn-row">
                    <button class="btn primary" on:click=move |_| export_json(app)><Icon name="download"/>"Export backup"</button>
                    <button class="btn" on:click=move |_| {
                        if let Some(i) = file_input.get_untracked() { i.click(); }
                    }><Icon name="upload"/>"Import backup"</button>
                    <button class="btn ghost" on:click=move |_| export_csv(app)><Icon name="file"/>"Export CSV"</button>
                </div>
                <input type="file" accept="application/json,.json" class="hidden" node_ref=file_input on:change=on_import/>
                <div class="danger-zone">
                    <button class="btn danger ghost" on:click=move |_| {
                        if store::confirm("Erase all leave data from this browser? Export a backup first if you might need it.") {
                            let before = app.data.get_untracked();
                            app.data.set(Data::default());
                            app.notify("All data erased", Some(before));
                        }
                    }><Icon name="trash"/>"Erase everything"</button>
                </div>
            </section>

            <section class="card reveal about" style="--i:6">
                <header class="card-head"><h3>"Shortcuts"</h3></header>
                <dl class="keys">
                    <div><dt><kbd>"N"</kbd></dt><dd>"Add leave"</dd></div>
                    <div><dt><kbd>"1"</kbd>"–"<kbd>"5"</kbd></dt><dd>"Switch section"</dd></div>
                    <div><dt><kbd>"Esc"</kbd></dt><dd>"Close sheet"</dd></div>
                    <div><dt><kbd>"←"</kbd><kbd>"→"</kbd></dt><dd>"Step through the chart"</dd></div>
                </dl>
            </section>
        </div>
    }
}
