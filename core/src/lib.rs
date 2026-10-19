//! Core leave maths, independent of any UI.

pub mod calendar;
pub mod engine;
pub mod model;

pub use calendar::WorkCalendar;
pub use engine::{compute, Report};
pub use model::*;
