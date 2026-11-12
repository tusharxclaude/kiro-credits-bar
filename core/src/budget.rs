//! Working days, daily budget and pace. Pure functions, no I/O.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Safety cap so a bad reset date can never make a loop run for years.
const MAX_CYCLE_DAYS: i64 = 400;

/// Which days count as working days: a weekly pattern minus the user's leave days.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub working_weekdays: BTreeSet<u8>, // 0 = Monday .. 6 = Sunday
    pub leave_days: BTreeSet<NaiveDate>,
}

impl Default for Schedule {
    /// Monday to Friday, no leave.
    fn default() -> Self {
        Schedule {
            working_weekdays: (0..5).collect(),
            leave_days: BTreeSet::new(),
        }
    }
}

impl Schedule {
    pub fn is_working(&self, day: NaiveDate) -> bool {
        let wd = day.weekday().num_days_from_monday() as u8;
        self.working_weekdays.contains(&wd) && !self.leave_days.contains(&day)
    }

    pub fn with_weekdays(days: &[Weekday]) -> Self {
        Schedule {
            working_weekdays: days
                .iter()
                .map(|d| d.num_days_from_monday() as u8)
                .collect(),
            leave_days: BTreeSet::new(),
        }
    }
}

/// Working days from `today` up to, but not including, `reset_on`. Counts today if it is a working day.
pub fn working_days_left(today: NaiveDate, reset_on: NaiveDate, schedule: &Schedule) -> u32 {
    let mut count = 0;
    let mut day = today;
    let mut steps = 0;
    while day < reset_on && steps < MAX_CYCLE_DAYS {
        if schedule.is_working(day) {
            count += 1;
        }
        day += Duration::days(1);
        steps += 1;
    }
    count
}

/// Credits to spend today, fixed for the day: opening balance divided by working days left.
/// A day off, or a cycle with no working days left, has a budget of 0.
pub fn daily_budget(
    opening_balance: f64,
    today: NaiveDate,
    reset_on: NaiveDate,
    schedule: &Schedule,
) -> f64 {
    if !schedule.is_working(today) {
        return 0.0;
    }
    let days = working_days_left(today, reset_on, schedule);
    if days == 0 {
        return 0.0;
    }
    opening_balance.max(0.0) / days as f64
}

/// Opening balance minus current remaining credits; never negative.
pub fn used_today(opening_balance: f64, remaining_now: f64) -> f64 {
    (opening_balance - remaining_now).max(0.0)
}

/// How today's usage compares with the daily budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pace {
    /// Up to 100% of the daily budget.
    OnTrack,
    /// Over 100% and up to 120%.
    Ahead,
    /// Over 120%.
    WellAhead,
}

pub const AMBER_ABOVE: f64 = 1.0;
pub const RED_ABOVE: f64 = 1.2;

pub fn pace(used_today: f64, budget: f64) -> Pace {
    if budget <= 0.0 {
        // A day off: any use at all is over budget.
        return if used_today > 0.0 {
            Pace::WellAhead
        } else {
            Pace::OnTrack
        };
    }
    let ratio = used_today / budget;
    if ratio > RED_ABOVE {
        Pace::WellAhead
    } else if ratio > AMBER_ABOVE {
        Pace::Ahead
    } else {
        Pace::OnTrack
    }
}

/// Credits left of today's budget (negative means over). Underspend is shown, never alarmed on.
pub fn budget_left_today(used_today: f64, budget: f64) -> f64 {
    budget - used_today
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn counts_working_days_in_the_real_cycle() {
        // Thu 12 Nov 2026 .. Mon 30 Nov 2026, reset on 1 Dec: 13 working days.
        let n = working_days_left(d(2026, 11, 12), d(2026, 12, 1), &Schedule::default());
        assert_eq!(n, 13);
    }

    #[test]
    fn reset_day_itself_is_not_part_of_the_cycle() {
        // Reset on a Tuesday: that Tuesday must not count.
        let n = working_days_left(d(2026, 11, 30), d(2026, 12, 1), &Schedule::default());
        assert_eq!(n, 1); // Monday 30th only
    }

    #[test]
    fn leave_days_remove_working_days() {
        let mut s = Schedule::default();
        s.leave_days.insert(d(2026, 11, 16));
        s.leave_days.insert(d(2026, 11, 17));
        assert_eq!(working_days_left(d(2026, 11, 12), d(2026, 12, 1), &s), 11);
    }

    #[test]
    fn leave_on_a_weekend_changes_nothing() {
        let mut s = Schedule::default();
        s.leave_days.insert(d(2026, 11, 14)); // Saturday
        assert_eq!(working_days_left(d(2026, 11, 12), d(2026, 12, 1), &s), 13);
    }

    #[test]
    fn working_week_is_the_users_choice() {
        let s = Schedule::with_weekdays(&[Weekday::Mon, Weekday::Tue, Weekday::Wed]);
        // 12 Nov..30 Nov: Mondays 16,23,30 + Tuesdays 17,24 + Wednesdays 18,25 = 7
        assert_eq!(working_days_left(d(2026, 11, 12), d(2026, 12, 1), &s), 7);
    }

    #[test]
    fn budget_matches_the_worked_example() {
        // 2000 - 349.90 remaining 1650.10 over 13 working days.
        let b = daily_budget(
            1650.10,
            d(2026, 11, 12),
            d(2026, 12, 1),
            &Schedule::default(),
        );
        assert!((b - 126.930_769).abs() < 1e-5, "got {b}");
    }

    #[test]
    fn overspending_lowers_tomorrows_budget_and_underspending_raises_it() {
        let s = Schedule::default();
        let reset = d(2026, 12, 1);
        let today_budget = daily_budget(1300.0, d(2026, 11, 18), reset, &s); // 9 days left
                                                                             // Overspend: used triple the budget today.
        let after_over = 1300.0 - today_budget * 3.0;
        let tomorrow_over = daily_budget(after_over, d(2026, 11, 19), reset, &s);
        // Underspend: used nothing today.
        let tomorrow_under = daily_budget(1300.0, d(2026, 11, 19), reset, &s);
        assert!(tomorrow_over < today_budget);
        assert!(tomorrow_under > today_budget);
    }

    #[test]
    fn a_day_off_has_a_zero_budget() {
        let b = daily_budget(
            1000.0,
            d(2026, 11, 14),
            d(2026, 12, 1),
            &Schedule::default(),
        ); // Saturday
        assert_eq!(b, 0.0);
    }

    #[test]
    fn credits_spent_on_a_day_off_reduce_the_next_working_days_budget() {
        let s = Schedule::default();
        let reset = d(2026, 12, 1);
        let monday_before = daily_budget(1000.0, d(2026, 11, 16), reset, &s);
        let monday_after_saturday_use = daily_budget(970.0, d(2026, 11, 16), reset, &s);
        assert!(monday_after_saturday_use < monday_before);
    }

    #[test]
    fn on_the_last_working_day_the_budget_is_everything_left() {
        let b = daily_budget(88.0, d(2026, 11, 30), d(2026, 12, 1), &Schedule::default());
        assert_eq!(b, 88.0);
    }

    #[test]
    fn a_stale_reset_date_in_the_past_gives_zero_budget_not_a_crash() {
        // Today is a working day but the reset date has already passed (not yet re-read).
        let b = daily_budget(50.0, d(2026, 11, 30), d(2026, 11, 30), &Schedule::default());
        assert_eq!(b, 0.0);
    }

    #[test]
    fn used_today_never_goes_negative() {
        assert_eq!(used_today(100.0, 120.0), 0.0); // e.g. a reading glitch
        assert!((used_today(100.0, 70.5) - 29.5).abs() < 1e-9);
    }

    #[test]
    fn pace_thresholds_are_green_amber_red() {
        assert_eq!(pace(0.0, 100.0), Pace::OnTrack);
        assert_eq!(pace(100.0, 100.0), Pace::OnTrack); // exactly 100% is still fine
        assert_eq!(pace(100.1, 100.0), Pace::Ahead);
        assert_eq!(pace(120.0, 100.0), Pace::Ahead); // exactly 120% is still amber
        assert_eq!(pace(120.1, 100.0), Pace::WellAhead);
    }

    #[test]
    fn any_use_on_a_day_off_is_red() {
        assert_eq!(pace(0.0, 0.0), Pace::OnTrack);
        assert_eq!(pace(0.01, 0.0), Pace::WellAhead);
    }

    #[test]
    fn underspending_is_never_alarming() {
        assert_eq!(pace(5.0, 100.0), Pace::OnTrack);
        assert_eq!(budget_left_today(5.0, 100.0), 95.0);
        assert!(budget_left_today(130.0, 100.0) < 0.0);
    }
}
