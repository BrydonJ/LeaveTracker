//! Balance-over-time line chart: hand-built SVG with a crosshair tooltip.

use std::sync::Arc;

use chrono::{Datelike, Days, Months, NaiveDate};
use leave_core::Report;
use leptos::ev;
use leptos::html;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;

use crate::fmt;
use crate::ui::Segmented;
use crate::use_app;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Span {
    Year,
    ThreeYears,
    All,
}

const H: f64 = 230.0;
const PAD_L: f64 = 34.0;
const PAD_R: f64 = 10.0;
const PAD_T: f64 = 14.0;
const PAD_B: f64 = 26.0;

#[derive(Clone, Debug, PartialEq)]
struct Geometry {
    from: NaiveDate,
    to: NaiveDate,
    today_x: Option<f64>,
    past: String,
    area: String,
    future: String,
    y_ticks: Vec<(f64, String)>,
    x_ticks: Vec<(f64, String)>,
    zero_y: f64,
    marks: Vec<(f64, f64)>,
    width: f64,
    y0: f64,
    y1: f64,
}

fn nice_step(range: f64) -> f64 {
    for s in [1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0] {
        if range / s <= 4.5 {
            return s;
        }
    }
    200.0
}

fn geometry(
    r: &Report,
    adj_dates: &[NaiveDate],
    today: NaiveDate,
    span: Span,
    width: f64,
) -> Option<Geometry> {
    let ahead = match span {
        Span::All => Months::new(12),
        _ => Months::new(6),
    };
    let to = (today.max(r.start) + ahead).min(r.horizon);
    let from = match span {
        Span::Year => today - Months::new(12),
        Span::ThreeYears => today - Months::new(36),
        Span::All => r.start,
    }
    .max(r.start);
    if to <= from {
        return None;
    }
    let n = (to - from).num_days() as usize + 1;
    let offset = (from - r.start).num_days() as usize;
    let values = &r.balances[offset..offset + n];

    let lo = values.iter().cloned().fold(0.0f64, f64::min);
    let hi = values.iter().cloned().fold(1.0f64, f64::max);
    let step = nice_step(hi - lo);
    let y0 = (lo / step).floor() * step;
    // Leave headroom above the highest value.
    let y1 = (hi / step).ceil() * step;
    let y1 = if y1 <= hi { y1 + step } else { y1 };

    let pw = (width - PAD_L - PAD_R).max(10.0);
    let ph = H - PAD_T - PAD_B;
    let x = |i: usize| PAD_L + pw * i as f64 / (n - 1).max(1) as f64;
    let y = |v: f64| PAD_T + ph * (1.0 - (v - y0) / (y1 - y0));

    let today_i = if today >= from && today <= to {
        Some((today - from).num_days() as usize)
    } else {
        None
    };
    let split = today_i.unwrap_or(if today > to { n - 1 } else { 0 });

    // Step path: horizontal run, then vertical change — balances move in discrete credits.
    let stride = ((n as f64 / pw).ceil() as usize).max(1);
    let step_path = |a: usize, b: usize| -> String {
        let mut s = format!("M{:.1},{:.1}", x(a), y(values[a]));
        let mut prev = values[a];
        let mut i = a + 1;
        while i <= b {
            let j = if i + stride > b { b } else { i };
            let v = values[j];
            if (v - prev).abs() > 1e-9 || j == b {
                s.push_str(&format!("H{:.1}V{:.1}", x(j), y(v)));
                prev = v;
            }
            if j == b {
                break;
            }
            i += stride;
        }
        s
    };

    let (past, area) = if today >= from {
        let p = step_path(0, split);
        let a = format!("{p}V{:.1}H{:.1}Z", y(0.0f64.max(y0)), x(0));
        (p, a)
    } else {
        (String::new(), String::new())
    };
    let future = if split < n - 1 {
        step_path(split, n - 1)
    } else {
        String::new()
    };

    let mut y_ticks = Vec::new();
    let mut v = y0;
    while v <= y1 + 1e-9 {
        y_ticks.push((y(v), fmt::days(v)));
        v += step;
    }

    let mut x_ticks = Vec::new();
    let span_days = n as f64;
    let month_step = if span_days <= 450.0 {
        if pw < 420.0 {
            3
        } else {
            2
        }
    } else if span_days <= 1200.0 {
        6
    } else {
        12
    };
    let mut m = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).unwrap();
    if m < from {
        m = m + Months::new(1);
    }
    while m <= to {
        if m.month0().is_multiple_of(month_step) {
            let label = if m.month() == 1 || month_step == 12 {
                m.year().to_string()
            } else {
                fmt::month_abbr(m.month()).to_string()
            };
            x_ticks.push((x((m - from).num_days() as usize), label));
        }
        m = m + Months::new(1);
    }

    let marks = adj_dates
        .iter()
        .filter(|d| **d >= from && **d <= to)
        .map(|d| {
            let i = (*d - from).num_days() as usize;
            (x(i), y(values[i]))
        })
        .collect();

    Some(Geometry {
        marks,
        from,
        to,
        today_x: today_i.map(x),
        past,
        area,
        future,
        y_ticks,
        x_ticks,
        zero_y: y(0.0),
        width,
        y0,
        y1,
    })
}

#[component]
pub fn BalanceChart() -> impl IntoView {
    let app = use_app();
    let span = RwSignal::new(Span::Year);
    let width = RwSignal::new(640.0f64);
    let wrap = NodeRef::<html::Div>::new();
    let hover = RwSignal::new(None::<usize>);

    let measure = move || {
        if let Some(el) = wrap.get_untracked() {
            let w = el.get_bounding_client_rect().width();
            if w > 0.0 && (w - width.get_untracked()).abs() > 1.0 {
                width.set(w);
            }
        }
    };
    Effect::new(move |_| {
        if wrap.get().is_some() {
            measure();
        }
    });
    let resize = window_event_listener(ev::resize, move |_| measure());
    on_cleanup(move || resize.remove());

    let geo = Memo::new(move |_| {
        let r: Arc<Report> = app.report.get()?;
        let adj: Vec<NaiveDate> = app
            .data
            .with(|d| d.adjustments.iter().map(|a| a.date).collect());
        geometry(&r, &adj, app.today.get(), span.get(), width.get())
    });

    let point = move |i: usize| -> Option<(f64, f64, NaiveDate, f64, bool)> {
        let g = geo.get()?;
        let r = app.report.get()?;
        let n = (g.to - g.from).num_days() as usize + 1;
        let i = i.min(n - 1);
        let d = g.from + Days::new(i as u64);
        let v = r.balance_on(d);
        let pw = g.width - PAD_L - PAD_R;
        let x = PAD_L + pw * i as f64 / (n - 1).max(1) as f64;
        let y = PAD_T + (H - PAD_T - PAD_B) * (1.0 - (v - g.y0) / (g.y1 - g.y0));
        Some((x, y, d, v, d > app.today.get()))
    };

    let on_move = move |e: ev::PointerEvent| {
        let Some(g) = geo.get_untracked() else { return };
        let Some(el) = wrap.get_untracked() else {
            return;
        };
        let rect = el.get_bounding_client_rect();
        let px = e.client_x() as f64 - rect.left();
        let n = (g.to - g.from).num_days() as f64 + 1.0;
        let pw = g.width - PAD_L - PAD_R;
        let i = (((px - PAD_L) / pw) * (n - 1.0))
            .round()
            .clamp(0.0, n - 1.0) as usize;
        hover.set(Some(i));
    };

    let on_key = move |e: ev::KeyboardEvent| {
        let Some(g) = geo.get_untracked() else { return };
        let n = (g.to - g.from).num_days() as usize + 1;
        let today_i = (app.today.get_untracked() - g.from)
            .num_days()
            .clamp(0, n as i64 - 1) as usize;
        let cur = hover.get_untracked().unwrap_or(today_i);
        let step = if e.shift_key() { 30 } else { 7 };
        let next = match e.key().as_str() {
            "ArrowLeft" => cur.saturating_sub(step),
            "ArrowRight" => (cur + step).min(n - 1),
            "Home" => 0,
            "End" => n - 1,
            "Escape" => {
                hover.set(None);
                return;
            }
            _ => return,
        };
        e.prevent_default();
        hover.set(Some(next));
    };

    view! {
        <header class="card-head">
            <div>
                <h3>"Balance over time"</h3>
                <p class="card-sub">"Solid is history · dashed is projected, incl. booked leave"</p>
            </div>
            <Segmented
                options=vec![(Span::Year, "1Y"), (Span::ThreeYears, "3Y"), (Span::All, "All")]
                value=span
                on_change=Callback::new(move |s| {
                    hover.set(None);
                    span.set(s)
                })
                label="Chart range"
            />
        </header>
        <div
            class="chart"
            node_ref=wrap
            tabindex="0"
            role="img"
            aria-label="Leave balance over time. Use arrow keys to inspect values."
            on:pointermove=on_move
            on:pointerdown=on_move
            on:pointerleave=move |_| hover.set(None)
            on:keydown=on_key
            on:blur=move |_| hover.set(None)
        >
            {move || geo.get().map(|g| {
                let w = g.width;
                view! {
                    <svg width=w height=H viewBox=format!("0 0 {w} {H}") class="chart-svg">
                        <defs>
                            <linearGradient id="area-grad" x1="0" y1="0" x2="0" y2="1">
                                <stop offset="0" class="grad-top"/>
                                <stop offset="1" class="grad-bottom"/>
                            </linearGradient>
                        </defs>
                        <g class="grid">
                            {g.y_ticks.iter().map(|(y, label)| view! {
                                <line x1=PAD_L x2=w - PAD_R y1=*y y2=*y/>
                                <text x=PAD_L - 8.0 y=*y + 4.0 text-anchor="end">{label.clone()}</text>
                            }).collect_view()}
                        </g>
                        <line class="baseline" x1=PAD_L x2=w - PAD_R y1=g.zero_y y2=g.zero_y/>
                        <g class="x-axis">
                            {g.x_ticks.iter().map(|(x, label)| view! {
                                <text x=*x y=H - 6.0 text-anchor="middle">{label.clone()}</text>
                            }).collect_view()}
                        </g>
                        <path class="area" d=g.area.clone()/>
                        <path class="line past" d=g.past.clone() pathLength="1"/>
                        <path class="line future" d=g.future.clone()/>
                        {g.marks.iter().map(|(x, y)| view! { <circle class="adj-mark" cx=*x cy=*y r="4"/> }).collect_view()}
                        {g.today_x.map(|x| view! {
                            <g class="today-mark">
                                <line x1=x x2=x y1=PAD_T - 4.0 y2=H - PAD_B/>
                                <text x=x y=PAD_T - 5.0 text-anchor={if x > w - 50.0 { "end" } else { "middle" }}>"Today"</text>
                            </g>
                        })}
                        {move || hover.get().and_then(point).map(|(x, y, _, _, _)| view! {
                            <g class="crosshair">
                                <line x1=x x2=x y1=PAD_T y2=H - PAD_B/>
                                <circle cx=x cy=y r="5"/>
                            </g>
                        })}
                    </svg>
                }
            })}
            {move || hover.get().and_then(point).map(|(x, _, d, v, future)| {
                let w = width.get();
                let left = x.clamp(90.0, w - 90.0);
                let adj = app.data.with(|data| {
                    data.adjustments.iter().filter(|a| a.date == d).map(|a| match a.kind {
                        leave_core::AdjustmentKind::SetBalance(b) => format!("Balance set to {}", fmt::days(b)),
                        leave_core::AdjustmentKind::Delta(x) => format!("Adjusted {}", fmt::days_signed(x)),
                    }).collect::<Vec<_>>()
                });
                view! {
                    <div class="tooltip" style=format!("left:{left}px")>
                        <strong>{format!("{} days", fmt::days(v))}</strong>
                        <span>{fmt::date_long(d)}{future.then_some(" · projected")}</span>
                        {adj.into_iter().map(|t| view! { <em>{t}</em> }).collect_view()}
                    </div>
                }
            })}
        </div>
    }
}
