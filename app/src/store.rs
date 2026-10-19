//! Browser persistence, file import/export and small DOM helpers.

use chrono::NaiveDate;
use leave_core::Data;
use wasm_bindgen::{JsCast, JsValue};

const DATA_KEY: &str = "leavetracker.data.v1";
const PREV_KEY: &str = "leavetracker.data.v1.prev";
const THEME_KEY: &str = "leavetracker.theme";
const BACKUP_KEY: &str = "leavetracker.lastBackup";
const CHANGE_KEY: &str = "leavetracker.lastChange";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

pub fn today() -> NaiveDate {
    let d = js_sys::Date::new_0();
    NaiveDate::from_ymd_opt(d.get_full_year() as i32, d.get_month() + 1, d.get_date())
        .expect("browser date is valid")
}

pub fn load() -> Data {
    let Some(s) = storage() else {
        return Data::default();
    };
    for key in [DATA_KEY, PREV_KEY] {
        if let Ok(Some(raw)) = s.get_item(key) {
            if let Ok(d) = Data::from_json(&raw) {
                return d;
            }
        }
    }
    Data::default()
}

pub fn save(d: &Data) {
    let Some(s) = storage() else { return };
    // Keep the previous good copy so a bad write can never lose everything.
    if let Ok(Some(old)) = s.get_item(DATA_KEY) {
        let _ = s.set_item(PREV_KEY, &old);
    }
    let _ = s.set_item(DATA_KEY, &d.to_json());
    let _ = s.set_item(CHANGE_KEY, &now_ms().to_string());
}

pub fn last_backup() -> Option<f64> {
    storage()?.get_item(BACKUP_KEY).ok()??.parse().ok()
}

pub fn last_change() -> Option<f64> {
    storage()?.get_item(CHANGE_KEY).ok()??.parse().ok()
}

pub fn mark_backup() -> f64 {
    let t = now_ms();
    if let Some(s) = storage() {
        let _ = s.set_item(BACKUP_KEY, &t.to_string());
    }
    t
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }
}

pub fn load_theme() -> Theme {
    match storage()
        .and_then(|s| s.get_item(THEME_KEY).ok().flatten())
        .as_deref()
    {
        Some("light") => Theme::Light,
        Some("dark") => Theme::Dark,
        _ => Theme::System,
    }
}

pub fn save_theme(t: Theme) {
    if let Some(s) = storage() {
        let _ = match t {
            Theme::System => s.remove_item(THEME_KEY),
            Theme::Light => s.set_item(THEME_KEY, "light"),
            Theme::Dark => s.set_item(THEME_KEY, "dark"),
        };
    }
}

pub fn apply_theme(t: Theme) {
    let Some(el) = document().document_element() else {
        return;
    };
    let _ = match t {
        Theme::System => el.remove_attribute("data-theme"),
        Theme::Light => el.set_attribute("data-theme", "light"),
        Theme::Dark => el.set_attribute("data-theme", "dark"),
    };
}

pub fn document() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}

/// Asks the browser not to evict our storage under pressure.
pub fn request_persistence() {
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().storage().persist();
    }
}

pub fn remove_boot_screen() {
    if let Some(el) = document().get_element_by_id("boot") {
        el.remove();
    }
}

/// Triggers a download of `contents` as a file.
pub fn download(filename: &str, mime: &str, contents: &str) {
    let parts = js_sys::Array::of1(&JsValue::from_str(contents));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    let doc = document();
    if let Ok(a) = doc.create_element("a") {
        let a: web_sys::HtmlAnchorElement = a.unchecked_into();
        a.set_href(&url);
        a.set_download(filename);
        if let Some(body) = doc.body() {
            let _ = body.append_child(&a);
            a.click();
            a.remove();
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

pub fn confirm(msg: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.confirm_with_message(msg).ok())
        .unwrap_or(false)
}

pub fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| {
            w.match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
        })
        .map(|m| m.matches())
        .unwrap_or(false)
}
