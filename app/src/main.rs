mod calendar_view;
mod chart;
mod fmt;
mod leave;
mod onboarding;
mod overview;
mod settings;
mod statement;
mod store;
mod ui;

use std::sync::Arc;

use chrono::NaiveDate;
use leave_core::{compute, Data, LeaveKind, Report};
use leptos::ev;
use leptos::leptos_dom::helpers::{set_timeout, window_event_listener};
use leptos::prelude::*;
use std::time::Duration;

use crate::leave::{Draft, EntrySheet};
use crate::ui::Icon;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    Leave,
    Calendar,
    Statement,
    Settings,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Overview,
        Tab::Leave,
        Tab::Calendar,
        Tab::Statement,
        Tab::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Leave => "Leave",
            Tab::Calendar => "Calendar",
            Tab::Statement => "Statement",
            Tab::Settings => "Settings",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tab::Overview => "home",
            Tab::Leave => "list",
            Tab::Calendar => "calendar",
            Tab::Statement => "receipt",
            Tab::Settings => "settings",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub id: u32,
    pub message: String,
    /// Snapshot to restore when the user taps Undo.
    pub undo: Option<Data>,
}

/// Global app state, shared through context. All fields are cheap `Copy` handles.
#[derive(Clone, Copy)]
pub struct AppState {
    pub data: RwSignal<Data>,
    pub today: RwSignal<NaiveDate>,
    pub report: Memo<Option<Arc<Report>>>,
    pub tab: RwSignal<Tab>,
    pub sheet: RwSignal<Option<Draft>>,
    pub toast: RwSignal<Option<Toast>>,
    pub theme: RwSignal<store::Theme>,
    pub last_backup: RwSignal<Option<f64>>,
}

impl AppState {
    pub fn notify(&self, message: impl Into<String>, undo: Option<Data>) {
        let id = self.toast.get_untracked().map(|t| t.id + 1).unwrap_or(1);
        self.toast.set(Some(Toast {
            id,
            message: message.into(),
            undo,
        }));
        let toast = self.toast;
        set_timeout(
            move || {
                if toast.get_untracked().map(|t| t.id) == Some(id) {
                    toast.set(None);
                }
            },
            Duration::from_millis(5200),
        );
    }

    /// Applies a change and offers an undo for it.
    pub fn mutate_with_undo(&self, message: &str, f: impl FnOnce(&mut Data)) {
        let before = self.data.get_untracked();
        self.data.update(f);
        self.notify(message, Some(before));
    }

    pub fn open_new(&self, start: Option<NaiveDate>, end: Option<NaiveDate>) {
        let t = self.today.get_untracked();
        let s = start.unwrap_or(t);
        self.sheet
            .set(Some(Draft::new(LeaveKind::Annual, s, end.unwrap_or(s))));
    }
}

pub fn use_app() -> AppState {
    expect_context::<AppState>()
}

#[component]
fn App() -> impl IntoView {
    let data = RwSignal::new(store::load());
    let today = RwSignal::new(store::today());
    let report = Memo::new(move |_| {
        let t = today.get();
        data.with(|d| compute(d, t, None).map(Arc::new))
    });
    let state = AppState {
        data,
        today,
        report,
        tab: RwSignal::new(Tab::Overview),
        sheet: RwSignal::new(None),
        toast: RwSignal::new(None),
        theme: RwSignal::new(store::load_theme()),
        last_backup: RwSignal::new(store::last_backup()),
    };
    provide_context(state);

    // Persist every change. The first run only records the loaded data.
    Effect::new(move |prev: Option<()>| {
        data.with(|d| {
            if prev.is_some() {
                store::save(d);
            }
        });
    });
    Effect::new(move |_| store::apply_theme(state.theme.get()));

    // Keep "today" correct if the app is left open past midnight.
    let _ = window_event_listener(ev::visibilitychange, move |_| {
        let t = store::today();
        if t != today.get_untracked() {
            today.set(t);
        }
    });

    // Global shortcuts: N = new leave, 1–5 = switch tab.
    let _ = window_event_listener(ev::keydown, move |e| {
        if e.ctrl_key() || e.meta_key() || e.alt_key() || state.sheet.get_untracked().is_some() {
            return;
        }
        if ui::typing_in_field(&e) || data.with_untracked(|d| d.settings.start_date.is_none()) {
            return;
        }
        match e.key().as_str() {
            "n" | "N" => {
                e.prevent_default();
                state.open_new(None, None);
            }
            k @ ("1" | "2" | "3" | "4" | "5") => {
                let i = k.parse::<usize>().unwrap() - 1;
                state.tab.set(Tab::ALL[i]);
            }
            _ => {}
        }
    });

    store::request_persistence();
    store::remove_boot_screen();

    let onboarded = move || data.with(|d| d.settings.start_date.is_some());

    view! {
        <Show when=onboarded fallback=|| view! { <onboarding::Onboarding/> }>
            <Shell/>
        </Show>
        <EntrySheet/>
        <ToastHost/>
    }
}

#[component]
fn Shell() -> impl IntoView {
    let app = use_app();
    let tab_index = move || {
        Tab::ALL
            .iter()
            .position(|t| *t == app.tab.get())
            .unwrap_or(0)
    };

    view! {
        <div class="shell">
            <header class="topbar">
                <div class="brand">
                    <span class="brand-mark" aria-hidden="true">
                        <Icon name="leaf"/>
                    </span>
                    <span class="brand-name">"Leave"</span>
                </div>
                <nav class="tabs" aria-label="Sections" style=move || format!("--tab:{}", tab_index())>
                    <span class="tab-pill" aria-hidden="true"></span>
                    {Tab::ALL
                        .into_iter()
                        .map(|t| {
                            view! {
                                <button
                                    class="tab"
                                    class:active=move || app.tab.get() == t
                                    aria-current=move || (app.tab.get() == t).then_some("page")
                                    on:click=move |_| {
                                        app.tab.set(t);
                                        ui::scroll_to_top();
                                    }
                                >
                                    <Icon name=t.icon()/>
                                    <span>{t.label()}</span>
                                </button>
                            }
                        })
                        .collect_view()}
                </nav>
                <div class="topbar-actions">
                    <ThemeToggle/>
                    <button class="btn primary add-btn" on:click=move |_| app.open_new(None, None) title="Add leave (N)">
                        <Icon name="plus"/>
                        <span>"Add leave"</span>
                    </button>
                </div>
            </header>
            <main class="content">
                {move || {
                    let view = match app.tab.get() {
                        Tab::Overview => view! { <overview::Overview/> }.into_any(),
                        Tab::Leave => view! { <leave::LeaveList/> }.into_any(),
                        Tab::Calendar => view! { <calendar_view::CalendarView/> }.into_any(),
                        Tab::Statement => view! { <statement::Statement/> }.into_any(),
                        Tab::Settings => view! { <settings::SettingsView/> }.into_any(),
                    };
                    view! { <div class="page">{view}</div> }
                }}
            </main>
            <button class="fab" on:click=move |_| app.open_new(None, None) aria-label="Add leave">
                <Icon name="plus"/>
            </button>
        </div>
    }
}

#[component]
fn ThemeToggle() -> impl IntoView {
    let app = use_app();
    let next = move || match app.theme.get() {
        store::Theme::System => store::Theme::Light,
        store::Theme::Light => store::Theme::Dark,
        store::Theme::Dark => store::Theme::System,
    };
    view! {
        <button
            class="icon-btn"
            title=move || format!("Theme: {}", app.theme.get().label())
            aria-label=move || format!("Theme: {}. Click to change.", app.theme.get().label())
            on:click=move |_| {
                let t = next();
                app.theme.set(t);
                store::save_theme(t);
            }
        >
            {move || {
                let name = match app.theme.get() {
                    store::Theme::System => "auto",
                    store::Theme::Light => "sun",
                    store::Theme::Dark => "moon",
                };
                view! { <Icon name=name/> }
            }}
        </button>
    }
}

#[component]
fn ToastHost() -> impl IntoView {
    let app = use_app();
    view! {
        <div class="toast-host" aria-live="polite">
            {move || {
                app.toast
                    .get()
                    .map(|t| {
                        let undo = t.undo.clone();
                        view! {
                            <div class="toast">
                                <span>{t.message.clone()}</span>
                                {undo
                                    .map(|snap| {
                                        view! {
                                            <button
                                                class="toast-action"
                                                on:click=move |_| {
                                                    app.data.set(snap.clone());
                                                    app.toast.set(None);
                                                }
                                            >
                                                "Undo"
                                            </button>
                                        }
                                    })}
                            </div>
                        }
                    })
            }}
        </div>
    }
}

fn main() {
    console_error_panic_hook_lite();
    leptos::mount::mount_to_body(App);
}

/// Surfaces panics in the console without pulling in an extra crate.
fn console_error_panic_hook_lite() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&info.to_string().into());
        // The app can't continue after a panic; say so rather than freezing silently.
        let doc = store::document();
        if let (Some(body), Ok(el)) = (doc.body(), doc.create_element("div")) {
            el.set_class_name("crash");
            el.set_inner_html(
                "<strong>Something went wrong.</strong><span>Your data is saved. Reload to continue.</span>\
                 <button onclick=\"location.reload()\">Reload</button>",
            );
            let _ = body.append_child(&el);
        }
    }));
}
