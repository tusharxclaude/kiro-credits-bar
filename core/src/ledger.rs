//! Per-day records, anchored daily allowances, carryover and the usage trend.
//!
//! Kiro only reports a cycle total, so "used today" needs the remaining credits
//! at the first reading of the day (the opening balance). That is stored here.

use crate::budget::{self, Pace, Schedule};
use crate::usage::UsageReading;
use chrono::{DateTime, NaiveDate, Utc};
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
    /// Usage between the previous recorded day and this day's first reading.
    #[serde(default)]
    pub unallocated_usage: f64,
    #[serde(default)]
    pub first_checked_at: Option<DateTime<Utc>>,
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
    pub base_allowance: f64,
    pub carryover: f64,
    pub unallocated_usage: f64,
    pub first_checked_at: Option<DateTime<Utc>>,
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
    #[serde(default)]
    pub last_checked_at: Option<DateTime<Utc>>,
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
        // Anchor to the earliest observed balance; never invent a cycle start balance.
        let (anchor_day, anchor_balance) = self
            .days
            .iter()
            .find(|(date, r)| **date <= today && r.reset_on == reading.reset_on)
            .map(|(date, r)| (*date, r.opening))
            .unwrap_or((today, opening));
        let anchor_days = budget::working_days_left(anchor_day, reading.reset_on, schedule);
        let base = if anchor_days > 0 {
            anchor_balance / anchor_days as f64
        } else {
            0.0
        };
        let days_left = budget::working_days_left(today, reading.reset_on, schedule);
        let carryover = opening - base * days_left as f64;
        let working = schedule.is_working(today) && days_left > 0;
        let base_allowance = if working { base } else { 0.0 };
        let budget = if working {
            (base + carryover).clamp(0.0, opening)
        } else {
            0.0
        };
        let existing = self
            .days
            .get(&today)
            .filter(|r| r.reset_on == reading.reset_on);
        let unallocated_usage = existing.map(|r| r.unallocated_usage).unwrap_or_else(|| {
            self.days
                .range(..today)
                .rev()
                .find(|(_, r)| r.reset_on == reading.reset_on)
                .map(|(_, r)| (r.latest_remaining - remaining).max(0.0))
                .unwrap_or(0.0)
        });
        let first_checked_at = existing.and_then(|r| r.first_checked_at);
        self.days.insert(
            today,
            DayRecord {
                reset_on: reading.reset_on,
                opening,
                latest_remaining: remaining,
                budget,
                unallocated_usage,
                first_checked_at,
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
            base_allowance,
            carryover: if working { carryover } else { 0.0 },
            unallocated_usage,
            first_checked_at,
            budget_left: budget::budget_left_today(used, budget),
            working_days_left: budget::working_days_left(today, reading.reset_on, schedule),
            pace: budget::pace(used, budget),
        }
    }

    /// Called only for a successful live read, never for a settings edit or stale view.
    pub fn checked(&mut self, today: NaiveDate, at: DateTime<Utc>) {
        self.last_checked_at = Some(at);
        if let Some(day) = self.days.get_mut(&today) {
            day.first_checked_at.get_or_insert(at);
        }
    }

    pub fn has_day(&self, today: NaiveDate) -> bool {
        self.days.contains_key(&today)
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
    fn carryover_matches_the_100_plus_or_minus_20_examples() {
        for (spent, expected) in [(80.0, 120.0), (120.0, 80.0), (0.0, 200.0)] {
            let mut ledger = Ledger::new();
            let schedule = Schedule::default();
            let start = d(2026, 11, 26); // Thu, Fri, Mon: three working days
            ledger.record(start, &reading(1700.0, RESET()), &schedule);
            ledger.record(start, &reading(1700.0 + spent, RESET()), &schedule);
            let mut restored = Ledger::from_json(&ledger.to_json()).unwrap();
            let next = restored.record(
                d(2026, 11, 27),
                &reading(1700.0 + spent, RESET()),
                &schedule,
            );
            assert!((next.base_allowance - 100.0).abs() < 1e-9);
            assert!((next.daily_budget - expected).abs() < 1e-9);
            assert!((next.carryover - (100.0 - spent)).abs() < 1e-9);
        }
    }

    #[test]
    fn offline_gap_reduces_carryover_without_inventing_daily_usage() {
        let mut ledger = Ledger::new();
        let s = Schedule::default();
        ledger.record(d(2026, 11, 26), &reading(1700.0, RESET()), &s);
        ledger.record(d(2026, 11, 26), &reading(1780.0, RESET()), &s);
        let friday = ledger.record(d(2026, 11, 27), &reading(1800.0, RESET()), &s);
        assert_eq!(friday.unallocated_usage, 20.0);
        assert_eq!(friday.used_today, 0.0);
        assert_eq!(friday.daily_budget, 100.0);
        let later = ledger.record(d(2026, 11, 27), &reading(1810.0, RESET()), &s);
        assert_eq!(later.unallocated_usage, 20.0);
        assert_eq!(later.used_today, 10.0);
    }

    #[test]
    fn weekend_has_no_base_and_debt_never_allows_negative_spending() {
        let mut ledger = Ledger::new();
        let s = Schedule::default();
        ledger.record(d(2026, 11, 26), &reading(1700.0, RESET()), &s);
        ledger.record(d(2026, 11, 26), &reading(1950.0, RESET()), &s);
        let fri = ledger.record(d(2026, 11, 27), &reading(1950.0, RESET()), &s);
        assert_eq!(fri.daily_budget, 0.0);
        assert_eq!(fri.carryover, -150.0);
        let sat = ledger.record(d(2026, 11, 28), &reading(1960.0, RESET()), &s);
        assert_eq!(sat.base_allowance, 0.0);
        assert_eq!(sat.daily_budget, 0.0);
        let mon = ledger.record(d(2026, 11, 30), &reading(1960.0, RESET()), &s);
        assert_eq!(mon.daily_budget, 40.0);
    }

    #[test]
    fn old_ledgers_load_without_timestamps_or_gap_fields() {
        let text = r#"{"days":{"2026-11-26":{"reset_on":"2026-12-01","opening":300.0,"latest_remaining":220.0,"budget":100.0}}}"#;
        let mut ledger = Ledger::from_json(text).unwrap();
        assert_eq!(ledger.last_checked_at, None);
        let next = ledger.record(
            d(2026, 11, 27),
            &reading(1780.0, RESET()),
            &Schedule::default(),
        );
        assert_eq!(next.daily_budget, 120.0);
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
