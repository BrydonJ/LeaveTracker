//! Persisted data model. Everything the user enters lives here; everything
//! else is derived by the engine, so the stored file stays small and auditable.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub version: u32,
    pub settings: Settings,
    pub entries: Vec<LeaveEntry>,
    pub adjustments: Vec<Adjustment>,
    /// Monotonic id source for entries and adjustments.
    pub next_id: u64,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            settings: Settings::default(),
            entries: Vec::new(),
            adjustments: Vec::new(),
            next_id: 1,
        }
    }
}

impl Data {
    pub fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("data is always serialisable")
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let mut d: Data = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if d.version > SCHEMA_VERSION {
            return Err(format!(
                "This file was made by a newer version (schema {}).",
                d.version
            ));
        }
        d.version = SCHEMA_VERSION;
        let max_id = d
            .entries
            .iter()
            .map(|e| e.id)
            .chain(d.adjustments.iter().map(|a| a.id))
            .max()
            .unwrap_or(0);
        d.next_id = d.next_id.max(max_id + 1);
        Ok(d)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreditTiming {
    /// The month's accrual is credited on the last day of the month.
    MonthEnd,
    /// Accrual is credited every day (balance moves daily).
    Daily,
    /// The whole cycle's allowance is credited on the first day of the cycle
    /// (pro-rated for a partial first cycle).
    Upfront,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateBasis {
    /// The extra day applies from the work anniversary itself.
    Anniversary,
    /// The extra day applies from the start of the next leave cycle
    /// (years completed on the first day of the cycle).
    CycleStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CycleBasis {
    /// Leave cycle runs from work anniversary to the day before the next.
    Anniversary,
    /// Leave cycle runs 1 Jan – 31 Dec.
    Calendar,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub start_date: Option<NaiveDate>,
    /// Annual entitlement in the first year of service.
    pub base_days: f64,
    /// Extra days per completed year of service.
    pub increment_per_year: f64,
    /// Optional ceiling on the annual entitlement.
    pub max_entitlement: Option<f64>,
    pub credit_timing: CreditTiming,
    pub rate_basis: RateBasis,
    pub cycle: CycleBasis,
    /// Maximum balance carried into a new cycle. `None` = unlimited.
    pub carry_cap: Option<f64>,
    /// Carried-over days must be used within this many months. `None` = never expire.
    pub carry_expiry_months: Option<u32>,
    pub sa_holidays: bool,
    pub custom_holidays: Vec<Holiday>,
    /// Working weekdays, Monday first.
    pub work_days: [bool; 7],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_date: None,
            base_days: 15.0,
            increment_per_year: 1.0,
            max_entitlement: None,
            credit_timing: CreditTiming::MonthEnd,
            rate_basis: RateBasis::Anniversary,
            cycle: CycleBasis::Anniversary,
            carry_cap: None,
            carry_expiry_months: None,
            sa_holidays: true,
            custom_holidays: Vec::new(),
            work_days: [true, true, true, true, true, false, false],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holiday {
    pub date: NaiveDate,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LeaveKind {
    Annual,
    Sick,
    Family,
    Study,
    Unpaid,
    Other,
}

impl LeaveKind {
    pub const ALL: [LeaveKind; 6] = [
        LeaveKind::Annual,
        LeaveKind::Sick,
        LeaveKind::Family,
        LeaveKind::Study,
        LeaveKind::Unpaid,
        LeaveKind::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LeaveKind::Annual => "Annual",
            LeaveKind::Sick => "Sick",
            LeaveKind::Family => "Family",
            LeaveKind::Study => "Study",
            LeaveKind::Unpaid => "Unpaid",
            LeaveKind::Other => "Other",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            LeaveKind::Annual => "annual",
            LeaveKind::Sick => "sick",
            LeaveKind::Family => "family",
            LeaveKind::Study => "study",
            LeaveKind::Unpaid => "unpaid",
            LeaveKind::Other => "other",
        }
    }

    /// Only annual leave reduces the annual leave balance.
    pub fn deducts(self) -> bool {
        matches!(self, LeaveKind::Annual)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeaveEntry {
    pub id: u64,
    pub kind: LeaveKind,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Single working day counted as half.
    #[serde(default)]
    pub half_day: bool,
    /// Manual override of the number of days charged.
    #[serde(default)]
    pub days_override: Option<f64>,
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AdjustmentKind {
    /// Add (positive) or remove (negative) days.
    Delta(f64),
    /// Force the balance to this value at the end of the day (e.g. an HR-confirmed figure).
    SetBalance(f64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Adjustment {
    pub id: u64,
    pub date: NaiveDate,
    pub kind: AdjustmentKind,
    #[serde(default)]
    pub note: String,
}
