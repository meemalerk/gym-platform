//! Use-cases and the ports they need. Depends on `gym-domain`; knows nothing about
//! HTTP or SQL. `gym-infrastructure` implements these ports.
//!
//! Dependency direction (docs/architecture.md): api → application → domain.

pub mod assignments;
pub mod auth;
pub mod billing;
pub mod calendar;
pub mod checkins;
pub mod classes;
pub mod coaching;
pub mod coaching_requests;
pub mod entitlements;
pub mod error;
pub mod execution;
pub mod exercises;
pub mod goals;
pub mod gyms;
pub mod ports;
pub mod profiles;
pub mod programs;
pub mod recommendations;

pub use error::{ApplicationError, ApplicationResult};
