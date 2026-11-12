//! Pure logic for Kiro Credits Bar. No UI, no network.
//!
//! - [`usage`]: parse `kiro-cli /usage` output
//! - [`budget`]: working days, daily budget, pace
//! - [`ledger`]: per-day opening balance, today's view, usage trend
//! - [`source`]: run the CLI safely (ADR 0001)
//! - [`engine`]: poll, manual entry, tray state, notify-once

pub mod budget;
pub mod ledger;
pub mod source;
pub mod usage;

pub mod engine;
