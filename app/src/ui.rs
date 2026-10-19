//! Shared widgets: icons, animated numbers, segmented controls, switches.

use leptos::leptos_dom::helpers::request_animation_frame;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::fmt;
use crate::store;

fn icon_paths(name: &str) -> &'static str {
    match name {
        "home" => {
            r#"<path d="M3 10.5 12 3l9 7.5"/><path d="M5 9v11h14V9"/><path d="M10 20v-5.5h4V20"/>"#
        }
        "list" => {
            r#"<path d="M9 6h11M9 12h11M9 18h11"/><path d="M4 6h.01M4 12h.01M4 18h.01" stroke-width="2.6"/>"#
        }
        "calendar" => {
            r#"<rect x="3.5" y="4.5" width="17" height="16" rx="3"/><path d="M3.5 9.5h17M8 2.75v3.5M16 2.75v3.5"/>"#
        }
        "receipt" => {
            r#"<path d="M6 3h12v18l-2.5-1.6L13 21l-2.5-1.6L8 21l-2-1.4V3z"/><path d="M9 8h6M9 12h6M9 16h3"/>"#
        }
        "settings" => {
            r#"<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2.2"/><circle cx="9" cy="17" r="2.2"/>"#
        }
        "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
        "sun" => {
            r#"<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M4.6 4.6l1.4 1.4M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4"/>"#
        }
        "moon" => r#"<path d="M20 14.2A8.2 8.2 0 1 1 9.8 4a6.4 6.4 0 0 0 10.2 10.2z"/>"#,
        "auto" => {
            r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 3.5v17a8.5 8.5 0 0 0 0-17z" fill="currentColor" stroke="none"/>"#
        }
        "leaf" => {
            r#"<path d="M4.5 19.5C4.5 10 10 4.5 20 4.5 20 14 14.5 19.5 6 19.5"/><path d="M4.5 19.5c2.6-4 6-7 10-9"/>"#
        }
        "download" => r#"<path d="M12 4v11M7.5 10.5 12 15l4.5-4.5M5 20h14"/>"#,
        "upload" => r#"<path d="M12 15.5V4.5M7.5 9 12 4.5 16.5 9M5 20h14"/>"#,
        "alert" => r#"<path d="M12 3.5 2.5 20h19z"/><path d="M12 10v4.5M12 17.2h.01"/>"#,
        "clock" => r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>"#,
        "x" => r#"<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>"#,
        "trash" => r#"<path d="M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13"/>"#,
        "check" => r#"<path d="M5 12.5 9.5 17 19 7.5"/>"#,
        "chev-left" => r#"<path d="M14.5 6 8.5 12l6 6"/>"#,
        "chev-right" => r#"<path d="M9.5 6l6 6-6 6"/>"#,
        "chev-down" => r#"<path d="M6 9.5l6 6 6-6"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>"#,
        "trend" => r#"<path d="M3.5 16.5 9 11l4 4 7.5-7.5"/><path d="M15 7.5h5.5V13"/>"#,
        "shield" => {
            r#"<path d="M12 3 19.5 6v5.5c0 4.6-3.2 7.9-7.5 9.5-4.3-1.6-7.5-4.9-7.5-9.5V6z"/><path d="m8.8 12 2.3 2.3 4.2-4.6"/>"#
        }
        "scale" => {
            r#"<path d="M12 4v16M6 20h12M4 8h16"/><path d="M4 8l-2 6a3 3 0 0 0 4 0zM20 8l-2 6a3 3 0 0 0 4 0z"/>"#
        }
        "file" => r#"<path d="M7 3h7l4 4v14H7z"/><path d="M14 3v4h4"/>"#,
        _ => "",
    }
}

#[component]
pub fn Icon(#[prop(into)] name: String) -> impl IntoView {
    view! {
        <svg
            class="icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
            inner_html=icon_paths(&name)
        ></svg>
    }
}

/// A number that eases to its new value whenever it changes.
#[component]
pub fn Counter(#[prop(into)] value: Signal<f64>) -> impl IntoView {
    // Remounts continue from the last value shown instead of counting up from zero again.
    let start = LAST_SHOWN.with(|c| c.get());
    let shown = RwSignal::new(start);
    let target = RwSignal::new(start);
    let generation = StoredValue::new(0u32);

    Effect::new(move |_| {
        let to = value.get();
        target.set(to);
        LAST_SHOWN.with(|c| c.set(to));
        let from = shown.get_untracked();
        let g = generation.get_value().wrapping_add(1);
        generation.set_value(g);
        if store::prefers_reduced_motion() || (to - from).abs() < 1e-9 {
            shown.set(to);
            return;
        }
        tween(store_now(), from, to, g, shown, generation);
    });

    view! {
        <span class="counter">{move || fmt::fixed(shown.get(), fmt::decimals_for(target.get()))}</span>
    }
}

thread_local! {
    static LAST_SHOWN: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn store_now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn tween(t0: f64, from: f64, to: f64, g: u32, shown: RwSignal<f64>, generation: StoredValue<u32>) {
    request_animation_frame(move || {
        // The counter may have been unmounted mid-animation.
        if generation.try_get_value() != Some(g) {
            return;
        }
        let t = ((store_now() - t0) / 900.0).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - t).powi(4);
        if shown.try_set(from + (to - from) * e).is_some() {
            return;
        }
        if t < 1.0 {
            tween(t0, from, to, g, shown, generation);
        }
    });
}

#[component]
pub fn Segmented<T>(
    options: Vec<(T, &'static str)>,
    #[prop(into)] value: Signal<T>,
    #[prop(into)] on_change: Callback<T>,
    #[prop(optional)] label: &'static str,
) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    let n = options.len();
    let opts = options.clone();
    let index = move || {
        let v = value.get();
        opts.iter().position(|(o, _)| *o == v).unwrap_or(0)
    };
    view! {
        <div
            class="segmented"
            role="radiogroup"
            aria-label=label
            style=move || format!("--n:{n};--i:{}", index())
        >
            <span class="seg-pill" aria-hidden="true"></span>
            {options
                .into_iter()
                .map(|(o, text)| {
                    view! {
                        <button
                            type="button"
                            role="radio"
                            class="seg"
                            class:active=move || value.get() == o
                            aria-checked=move || (value.get() == o).to_string()
                            on:click=move |_| on_change.run(o)
                        >
                            {text}
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
pub fn Switch(
    #[prop(into)] checked: Signal<bool>,
    #[prop(into)] on_change: Callback<bool>,
    #[prop(optional)] label: &'static str,
) -> impl IntoView {
    view! {
        <button
            type="button"
            role="switch"
            class="switch"
            class:on=move || checked.get()
            aria-checked=move || checked.get().to_string()
            aria-label=label
            on:click=move |_| on_change.run(!checked.get_untracked())
        >
            <span class="knob"></span>
        </button>
    }
}

pub fn typing_in_field(e: &web_sys::KeyboardEvent) -> bool {
    e.target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .map(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"))
        .unwrap_or(false)
}

pub fn scroll_to_top() {
    if let Some(w) = web_sys::window() {
        w.scroll_to_with_x_and_y(0.0, 0.0);
    }
}

/// Parses a number typed by a person ("1,5" is accepted as 1.5).
pub fn parse_num(s: &str) -> Option<f64> {
    s.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}
