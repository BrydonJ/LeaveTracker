# Leave Tracker

A fast, private leave tracker that tells you your exact leave balance and shows how it got there, day by day, so you don't have to ask HR.

Built in Rust: [Leptos](https://leptos.dev) compiled to WebAssembly, with an accrual engine that has no UI dependencies and is covered by unit tests.

## Features

- **Live balance** showing what's available today, what's still accruing, and what's left after leave you've already booked.
- **Entitlement rules:** a base allowance plus a set number of days per year of service (default 15 + 1), with an optional cap.
- **Accrual modes:**
  - credited monthly (at month end), daily, or upfront at the start of the cycle;
  - cycle runs from your work anniversary or by calendar year;
  - yearly increase applies on the anniversary itself or from the next cycle.
- **Carry-over:** an optional cap and an optional expiry ("use within N months"). Carried days are used first, and the app warns you before days are forfeited.
- **Working-day counting:** weekends and South African public holidays are excluded, including the Sunday → Monday rule and special declared days (election days and similar). You can add your own days off.
- **Back-capture:** "Save & add another" for entering past leave quickly, half days, manual day-count overrides, and an opening "balance confirmed by HR" if you'd rather not capture years of history.
- **Statement:** per-cycle and per-month ledger (opening, earned, taken, adjusted, forfeited, closing).
- **Check against HR:** type the figure HR gave you and see exactly how it compares with your record.
- **Calendar:** year view; drag across days to book leave.
- **Your data stays on your device:** it's stored in the browser and never sent anywhere. JSON backup/restore and CSV export are built in.
- Works offline (service worker), installable as an app on your phone, light and dark themes, keyboard shortcuts.

## Develop

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
cargo test -p leave-core      # accrual engine tests
cd app && trunk serve --open  # dev server with live reload
```

`trunk build --release` writes an optimised static site to `app/dist`.

## Deploy

Every push to `main` runs the tests, builds the site and publishes it to GitHub Pages using `.github/workflows/deploy.yml`. To set this up the first time, go to **Settings → Pages → Build and deployment → Source** and choose **GitHub Actions**.

## Layout

```
core/   leave-core: data model, SA holiday calendar, ledger engine (+ tests)
app/    leave-app: Leptos UI, styles, PWA assets
```

### How a balance is calculated

The engine starts at your start date and replays every day in order. For each day it:

1. expires any carried-over days that are past their use-by date;
2. accrues the day's leave (1/12 of the annual rate, spread over the month's days) and credits it according to your crediting setting;
3. applies any adjustments;
4. deducts annual leave taken that day (working days only);
5. on the last day of a cycle, applies the carry-over cap and moves the remaining balance into a carried-over lot.

The balance on any date is the sum of those steps, and the Statement tab shows each one.
