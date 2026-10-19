//! First-run setup.

use leave_core::{Adjustment, AdjustmentKind, CycleBasis, Data};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::fmt;
use crate::ui::{parse_num, Icon, Segmented};
use crate::use_app;

#[component]
pub fn Onboarding() -> impl IntoView {
    let app = use_app();
    let start = RwSignal::new(String::new());
    let base = RwSignal::new("15".to_string());
    let inc = RwSignal::new("1".to_string());
    let cycle = RwSignal::new(CycleBasis::Anniversary);
    let known = RwSignal::new(String::new());
    let known_date = RwSignal::new(fmt::iso(app.today.get_untracked()));
    let file_input = NodeRef::<leptos::html::Input>::new();

    let valid = move || {
        fmt::parse_iso(&start.get()).is_some()
            && parse_num(&base.get()).is_some()
            && parse_num(&inc.get()).is_some()
    };

    let begin = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        if !valid() {
            return;
        }
        app.data.update(|d| {
            d.settings.start_date = fmt::parse_iso(&start.get_untracked());
            d.settings.base_days = parse_num(&base.get_untracked()).unwrap_or(15.0);
            d.settings.increment_per_year = parse_num(&inc.get_untracked()).unwrap_or(1.0);
            d.settings.cycle = cycle.get_untracked();
            if let (Some(v), Some(at)) = (
                parse_num(&known.get_untracked()),
                fmt::parse_iso(&known_date.get_untracked()),
            ) {
                let id = d.take_id();
                d.adjustments.push(Adjustment {
                    id,
                    date: at,
                    kind: AdjustmentKind::SetBalance(v),
                    note: "Opening balance from HR".into(),
                });
            }
        });
    };

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
        spawn_local(async move {
            let Ok(text) = JsFuture::from(file.text()).await else {
                return;
            };
            match Data::from_json(&text.as_string().unwrap_or_default()) {
                Ok(d) => {
                    app.data.set(d);
                    app.notify("Backup restored", None);
                }
                Err(err) => app.notify(format!("Not a valid backup: {err}"), None),
            }
        });
    };

    view! {
        <div class="onboard">
            <form class="onboard-card" on:submit=begin>
                <div class="onboard-mark"><Icon name="leaf"/></div>
                <h1>"Know your leave."</h1>
                <p class="lede">"Your own exact, auditable leave balance — no more asking HR. Set up takes 20 seconds."</p>

                <label class="field">
                    <span>"When did you join the company?"</span>
                    <input type="date" required prop:value=start on:input=move |e| start.set(event_target_value(&e))/>
                </label>
                <div class="form-row">
                    <label class="field">
                        <span>"Days per year to start"</span>
                        <input type="text" inputmode="decimal" prop:value=base on:input=move |e| base.set(event_target_value(&e))/>
                    </label>
                    <label class="field">
                        <span>"+ per year worked"</span>
                        <input type="text" inputmode="decimal" prop:value=inc on:input=move |e| inc.set(event_target_value(&e))/>
                    </label>
                </div>
                <div class="setting">
                    <div class="opt-title">"Leave cycle"</div>
                    <Segmented
                        options=vec![(CycleBasis::Anniversary, "Work anniversary"), (CycleBasis::Calendar, "Calendar year")]
                        value=cycle
                        on_change=Callback::new(move |c| cycle.set(c))
                        label="Leave cycle"
                    />
                </div>
                <details class="opt-details">
                    <summary>"Have a balance from HR? Start from it"<Icon name="chev-down"/></summary>
                    <p class="hint">"Skip back-capturing years of history: enter a figure HR confirmed, then only capture leave after that date."</p>
                    <div class="form-row">
                        <label class="field">
                            <span>"Balance (days)"</span>
                            <input type="text" inputmode="decimal" placeholder="e.g. 7.5" prop:value=known on:input=move |e| known.set(event_target_value(&e))/>
                        </label>
                        <label class="field">
                            <span>"As at end of"</span>
                            <input type="date" prop:value=known_date on:input=move |e| known_date.set(event_target_value(&e))/>
                        </label>
                    </div>
                </details>
                <button class="btn primary big" type="submit" disabled=move || !valid()>"Start tracking"<Icon name="chev-right"/></button>
                <button type="button" class="link center" on:click=move |_| {
                    if let Some(i) = file_input.get_untracked() { i.click(); }
                }>"Restore from a backup instead"</button>
                <input type="file" accept="application/json,.json" class="hidden" node_ref=file_input on:change=on_import/>
                <p class="fineprint">"Defaults match South African practice: 15 working days, SA public holidays excluded. Everything is adjustable later."</p>
            </form>
        </div>
    }
}
