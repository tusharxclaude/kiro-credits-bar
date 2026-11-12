//! Per-day records: the opening balance, today's view and the usage trend.
//!
//! Kiro only reports a cycle total, so "used today" needs the remaining credits
//! at the first reading of the day (the opening balance). That is stored here.

use crate::budget::{self, Pace, Schedule};
use crate::usage::UsageReading;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayRecord {
    /// The billing cycle this day belongs to (identified by its reset date).
    pub reset_on: NaiveDate,
    /// Remaining credits at the first reading of the day. Never changes afterwards.
    pub opening: f64,
    /// Remaining credits at the latest reading of the day.
    pub latest_remaining: f64,
    /// Daily budget as last computed (follows schedule edits during the day).
    pub budget: f64,
}

impl DayRecord {
    pub fn used(&self) -> f64 {
        budget::used_today(self.opening, self.latest_remaining)
    }
}

/// Everything the tray and panel show for today.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayView {
    pub plan: String,
    pub allowance: f64,
    pub reset_on: NaiveDate,
    pub opening_balance: f64,
    pub remaining: f64,
    pub used_today: f64,
    pub daily_budget: f64,
    /// Budget minus used today; negative means over budget.
    pub budget_left: f64,
    pub working_days_left: u32,
    pub pace: Pace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendPoint {
    pub date: NaiveDate,
    pub used: f64,
    pub budget: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    days: BTreeMap<NaiveDate, DayRecord>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a fresh reading for `today` and return the view of the day.
    ///
    /// The first reading of a day (or the first of a new billing cycle) sets the
    /// opening balance. Later readings only move the latest remaining credits.
    pub fn record(
        &mut self,
        today: NaiveDate,
        reading: &UsageReading,
        schedule: &Schedule,
    ) -> DayView {
        let remaining = reading.remaining();
        let opening = match self.days.get(&today) {
            Some(existing) if existing.reset_on == reading.reset_on => existing.opening,
            _ => remaining, // first reading of the day, or the cycle just rolled over
        };
        let budget = budget::daily_budget(opening, today, reading.reset_on, schedule);
        self.days.insert(
            today,
            DayRecord {
                reset_on: reading.reset_on,
                opening,
                latest_remaining: remaining,
                budget,
            },
        );

        let used = budget::used_today(opening, remaining);
        DayView {
            plan: reading.plan.clone(),
            allowance: reading.total,
            reset_on: reading.reset_on,
            opening_balance: opening,
            remaining,
            used_today: used,
            daily_budget: budget,
            budget_left: budget::budget_left_today(used, budget),
            working_days_left: budget::working_days_left(today, reading.reset_on, schedule),
            pace: budget::pace(used, budget),
        }
    }

    /// One point per recorded day of the billing cycle that ends on `reset_on`.
    /// Days with no readings are simply absent (the app was not running, or it was a day off).
    pub fn trend(&self, reset_on: NaiveDate) -> Vec<TrendPoint> {
        self.days
            .iter()
            .filter(|(_, r)| r.reset_on == reset_on)
            .map(|(date, r)| TrendPoint {
                date: *date,
                used: r.used(),
                budget: r.budget,
            })
            .collect()
    }

    /// Keep only days from the last `keep` cycles' worth of records, to bound file size.
    pub fn prune_before(&mut self, cutoff: NaiveDate) {
        self.days.retain(|date, _| *date >= cutoff);
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ledger is always serializable")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn reading(used: f64, reset: NaiveDate) -> UsageReading {
        UsageReading {
            plan: "KIRO PRO+".into(),
            reset_on: reset,
            used,
            total: 2000.0,
        }
    }

    const RESET: fn() -> NaiveDate = || NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();

    #[test]
    fn first_reading_of_the_day_sets_the_opening_balance_and_budget() {
        let mut l = Ledger::new();
        let v = l.record(
            d(2026, 11, 12),
            &reading(349.90, RESET()),
            &Schedule::default(),
        );
        assert!((v.opening_balance - 1650.10).abs() < 1e-9);
        assert_eq!(v.used_today, 0.0);
        assert!((v.daily_budget - 126.930_769).abs() < 1e-5);
        assert_eq!(v.working_days_left, 13);
        assert_eq!(v.pace, Pace::OnTrack);
    }

    #[test]
    fn later_readings_fill_the_bar_without_moving_the_budget() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        let first = l.record(d(2026, 11, 12), &reading(349.90, RESET()), &s);
        let later = l.record(d(2026, 11, 12), &reading(349.90 + 63.0, RESET()), &s);
        assert_eq!(later.daily_budget, first.daily_budget); // fixed for the day
        assert!((later.used_today - 63.0).abs() < 1e-9);
        assert!((later.budget_left - (first.daily_budget - 63.0)).abs() < 1e-9);
    }

    #[test]
    fn pace_turns_amber_then_red_as_credits_are_used() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        l.record(d(2026, 11, 12), &reading(0.0, RESET()), &s); // budget 2000/13
        let budget = 2000.0 / 13.0;
        let amber = l.record(d(2026, 11, 12), &reading(budget * 1.1, RESET()), &s);
        let red = l.record(d(2026, 11, 12), &reading(budget * 1.3, RESET()), &s);
        assert_eq!(amber.pace, Pace::Ahead);
        assert_eq!(red.pace, Pace::WellAhead);
    }

    #[test]
    fn tomorrows_budget_absorbs_todays_overspend() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        let today = l.record(d(2026, 11, 12), &reading(0.0, RESET()), &s);
        l.record(
            d(2026, 11, 12),
            &reading(today.daily_budget * 2.0, RESET()),
            &s,
        );
        let tomorrow = l.record(
            d(2026, 11, 13),
            &reading(today.daily_budget * 2.0, RESET()),
            &s,
        );
        assert!(tomorrow.daily_budget < today.daily_budget);
        assert_eq!(tomorrow.used_today, 0.0); // new day starts empty
    }

    #[test]
    fn a_skipped_day_just_raises_the_next_budget() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        let thu = l.record(d(2026, 11, 12), &reading(100.0, RESET()), &s);
        // No readings on Friday 13th (unused or app off); Monday sees the same cycle total.
        let mon = l.record(d(2026, 11, 16), &reading(100.0, RESET()), &s);
        assert!(mon.daily_budget > thu.daily_budget);
    }

    #[test]
    fn using_credits_on_a_day_off_shows_red_and_shrinks_monday() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        l.record(d(2026, 11, 14), &reading(500.0, RESET()), &s); // Saturday, opening
        let sat = l.record(d(2026, 11, 14), &reading(530.0, RESET()), &s);
        assert_eq!(sat.daily_budget, 0.0);
        assert_eq!(sat.pace, Pace::WellAhead);
        let mon_without = Ledger::new().record(d(2026, 11, 16), &reading(500.0, RESET()), &s);
        let mon_with = l.record(d(2026, 11, 16), &reading(530.0, RESET()), &s);
        assert!(mon_with.daily_budget < mon_without.daily_budget);
    }

    #[test]
    fn a_billing_cycle_rollover_starts_a_fresh_opening_balance() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        l.record(d(2026, 11, 30), &reading(1900.0, RESET()), &s);
        // 1 Dec: credits renewed, new reset date, used back to ~0.
        let new_reset = d(2027, 1, 1);
        let v = l.record(d(2026, 12, 1), &reading(0.0, new_reset), &s);
        assert_eq!(v.opening_balance, 2000.0);
        assert_eq!(v.used_today, 0.0);
    }

    #[test]
    fn a_rollover_on_the_same_calendar_day_resets_that_days_opening() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        let day = d(2026, 12, 1);
        l.record(day, &reading(1950.0, RESET()), &s); // stale reading, old cycle
        let v = l.record(day, &reading(0.0, d(2027, 1, 1)), &s); // renewed
        assert_eq!(v.opening_balance, 2000.0);
        assert_eq!(v.used_today, 0.0);
    }

    #[test]
    fn schedule_edits_during_the_day_update_the_budget_but_not_the_opening() {
        let mut l = Ledger::new();
        let mut s = Schedule::default();
        let before = l.record(d(2026, 11, 12), &reading(0.0, RESET()), &s);
        s.leave_days.insert(d(2026, 11, 13)); // one fewer working day left
        let after = l.record(d(2026, 11, 12), &reading(0.0, RESET()), &s);
        assert_eq!(after.opening_balance, before.opening_balance);
        assert!(after.daily_budget > before.daily_budget);
    }

    #[test]
    fn trend_has_one_point_per_recorded_day_of_the_current_cycle_only() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        l.record(d(2026, 10, 30), &reading(1000.0, d(2026, 11, 1)), &s); // previous cycle
        l.record(d(2026, 11, 12), &reading(100.0, RESET()), &s);
        l.record(d(2026, 11, 12), &reading(160.0, RESET()), &s);
        l.record(d(2026, 11, 16), &reading(160.0, RESET()), &s);
        let t = l.trend(RESET());
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].date, d(2026, 11, 12));
        assert!((t[0].used - 60.0).abs() < 1e-9);
        assert_eq!(t[1].used, 0.0);
    }

    #[test]
    fn survives_a_save_and_load_round_trip() {
        let mut l = Ledger::new();
        l.record(
            d(2026, 11, 12),
            &reading(349.90, RESET()),
            &Schedule::default(),
        );
        let restored = Ledger::from_json(&l.to_json()).unwrap();
        assert_eq!(restored, l);
        assert!(Ledger::from_json("not json").is_err());
    }

    #[test]
    fn pruning_drops_old_days() {
        let mut l = Ledger::new();
        let s = Schedule::default();
        l.record(d(2026, 9, 1), &reading(10.0, d(2026, 10, 1)), &s);
        l.record(d(2026, 11, 12), &reading(10.0, RESET()), &s);
        l.prune_before(d(2026, 11, 1));
        assert!(l.trend(d(2026, 10, 1)).is_empty());
        assert_eq!(l.trend(RESET()).len(), 1);
    }
}
