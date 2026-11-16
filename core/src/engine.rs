//! Orchestration: poll, record, manual entry, tray state and the notify-once rule.
//!
//! The Tauri app owns one [`Engine`] and only translates its [`Update`]s into
//! tray icons, windows and notifications. Everything decidable lives here, tested.

use crate::budget::{Pace, Schedule};
use crate::ledger::{DayView, Ledger, TrendPoint};
use crate::source::{PollOutcome, Poller, UsageSource};
use crate::usage::UsageReading;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Persisted user choices.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schedule: Schedule,
    /// False until the user has confirmed their working week once.
    pub setup_done: bool,
    /// Explicit path to `kiro-cli`; otherwise it is searched for.
    pub cli_path: Option<String>,
}

/// Typed in by hand when Kiro cannot be read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManualEntry {
    pub reset_on: NaiveDate,
    pub allowance: f64,
    pub remaining: f64,
}

/// Why the numbers on screen may be out of date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reason")]
pub enum Notice {
    /// The CLI is missing or logged out. Retries on the next poll.
    Unavailable(String),
    /// Polling stopped after a surprise (ADR 0001). Needs the user to resume.
    Halted(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarDay {
    pub date: NaiveDate,
    /// 0 = Monday .. 6 = Sunday.
    pub weekday: u8,
    pub working: bool,
    pub leave: bool,
}

/// Everything the panel renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub today: NaiveDate,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub view: Option<DayView>,
    pub notice: Option<Notice>,
    /// True when the numbers came from manual entry rather than Kiro.
    pub manual: bool,
    pub trend: Vec<TrendPoint>,
    /// Days from today up to the reset date, for the calendar.
    pub calendar: Vec<CalendarDay>,
    pub settings: Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tone {
    Green,
    Amber,
    Red,
    /// No reading yet, or the reading is stale.
    Grey,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrayState {
    /// Short text such as `142 / 180`.
    pub label: String,
    pub tone: Tone,
    pub tooltip: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub snapshot: Snapshot,
    pub tray: TrayState,
    /// True exactly once per day, when used today first goes over the daily budget.
    pub notify: bool,
}

pub struct Engine<S: UsageSource> {
    poller: Poller<S>,
    settings: Settings,
    ledger: Ledger,
    last_reading: Option<UsageReading>,
    manual: bool,
    notice: Option<Notice>,
    notified_on: Option<NaiveDate>,
}

fn credits(n: f64) -> String {
    format!("{:.0}", n)
}

impl<S: UsageSource> Engine<S> {
    pub fn new(source: S, settings: Settings, ledger: Ledger) -> Self {
        Engine {
            poller: Poller::new(source),
            settings,
            ledger,
            last_reading: None,
            manual: false,
            notice: None,
            notified_on: None,
        }
    }

    pub fn source_mut(&mut self) -> &mut S {
        self.poller.source_mut()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Poll Kiro once (a no-op that costs nothing while halted).
    pub fn tick(&mut self, today: NaiveDate) -> Update {
        match self.poller.poll() {
            PollOutcome::Reading(reading) => {
                self.notice = None;
                self.manual = false;
                self.ledger.record(today, &reading, &self.settings.schedule);
                self.ledger.checked(today, Utc::now());
                self.last_reading = Some(reading);
            }
            PollOutcome::Unavailable(why) => self.notice = Some(Notice::Unavailable(why)),
            PollOutcome::Halted(why) => self.notice = Some(Notice::Halted(why)),
        }
        self.update(today)
    }

    /// The user pressed Resume after fixing the cause.
    pub fn resume(&mut self) {
        self.poller.resume();
        self.notice = None;
    }

    /// Inject a hand-typed reading. Kiro, if it becomes readable, overrides it.
    pub fn apply_manual(&mut self, today: NaiveDate, entry: ManualEntry) -> Update {
        let allowance = entry.allowance.max(1.0);
        let remaining = entry.remaining.clamp(0.0, allowance);
        self.last_reading = Some(UsageReading {
            plan: "MANUAL ENTRY".into(),
            reset_on: entry.reset_on,
            used: allowance - remaining,
            total: allowance,
        });
        self.ledger.record(
            today,
            self.last_reading.as_ref().unwrap(),
            &self.settings.schedule,
        );
        self.manual = true;
        self.update(today)
    }

    /// Change working weekdays and leave days; today's budget follows, the opening balance does not.
    pub fn set_schedule(
        &mut self,
        today: NaiveDate,
        schedule: Schedule,
        setup_done: bool,
    ) -> Update {
        self.settings.schedule = schedule;
        self.settings.setup_done = setup_done;
        self.update(today)
    }

    pub fn set_cli_path(&mut self, path: Option<String>) {
        self.settings.cli_path = path;
    }

    /// Recompute the snapshot from what is known, without touching Kiro.
    pub fn update(&mut self, today: NaiveDate) -> Update {
        let view = self.last_reading.clone().map(|r| {
            if self.notice.is_none() && self.ledger.has_day(today) {
                self.ledger.record(today, &r, &self.settings.schedule)
            } else {
                self.ledger
                    .clone()
                    .record(today, &r, &self.settings.schedule)
            }
        });

        let notify = match &view {
            Some(v) if !self.manual && self.notice.is_none() && v.pace != Pace::OnTrack => {
                let first_today = self.notified_on != Some(today);
                if first_today {
                    self.notified_on = Some(today);
                }
                first_today
            }
            _ => false,
        };

        let (trend, calendar) = match &view {
            Some(v) => (
                self.ledger.trend(v.reset_on),
                calendar(today, v.reset_on, &self.settings.schedule),
            ),
            None => (Vec::new(), Vec::new()),
        };
        let tray = tray_state(view.as_ref(), self.notice.as_ref(), self.manual);
        Update {
            snapshot: Snapshot {
                today,
                last_checked_at: self.ledger.last_checked_at,
                view,
                notice: self.notice.clone(),
                manual: self.manual,
                trend,
                calendar,
                settings: self.settings.clone(),
            },
            tray,
            notify,
        }
    }
}

/// Days from `today` up to (not including) `reset_on`.
pub fn calendar(today: NaiveDate, reset_on: NaiveDate, schedule: &Schedule) -> Vec<CalendarDay> {
    let mut days = Vec::new();
    let mut day = today;
    while day < reset_on && days.len() < 400 {
        days.push(CalendarDay {
            date: day,
            weekday: day.weekday().num_days_from_monday() as u8,
            working: schedule.is_working(day),
            leave: schedule.leave_days.contains(&day),
        });
        day += Duration::days(1);
    }
    days
}

pub fn tray_state(view: Option<&DayView>, notice: Option<&Notice>, manual: bool) -> TrayState {
    let Some(v) = view else {
        let why = match notice {
            Some(Notice::Halted(r)) | Some(Notice::Unavailable(r)) => r.clone(),
            None => "waiting for the first reading".to_string(),
        };
        return TrayState {
            label: "-- / --".into(),
            tone: Tone::Grey,
            tooltip: format!("Kiro credits: no reading yet ({why})"),
        };
    };
    let stale = notice.is_some();
    let tone = if stale {
        Tone::Grey
    } else {
        match v.pace {
            Pace::OnTrack => Tone::Green,
            Pace::Ahead => Tone::Amber,
            Pace::WellAhead => Tone::Red,
        }
    };
    let label = format!("{} / {}", credits(v.used_today), credits(v.daily_budget));
    let mut tooltip = format!(
        "Used today {} of {} credits. {} credits left in the cycle, {} working days to go.",
        credits(v.used_today),
        credits(v.daily_budget),
        credits(v.remaining),
        v.working_days_left
    );
    if stale {
        tooltip.push_str(" (last reading, Kiro not reachable)");
    } else if manual {
        tooltip.push_str(" (manual entry)");
    }
    TrayState {
        label,
        tone,
        tooltip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceError;
    use std::cell::Cell;
    use std::rc::Rc;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn text(used: f64) -> String {
        format!(
            "Estimated Usage | resets on 2026-12-01 | KIRO PRO+\nCredits ({used:.2} of 2000 covered in plan), 1.0%\nOverages: Disabled\n"
        )
    }

    struct Fake {
        used: Rc<Cell<f64>>,
        mode: Rc<Cell<u8>>, // 0 ok, 1 missing, 2 garbage
        calls: Rc<Cell<u32>>,
    }

    impl UsageSource for Fake {
        fn read(&mut self) -> Result<String, SourceError> {
            self.calls.set(self.calls.get() + 1);
            match self.mode.get() {
                0 => Ok(text(self.used.get())),
                1 => Err(SourceError::NotInstalled("not found".into())),
                _ => Ok("Run Kiro CLI and type /usage".into()),
            }
        }
    }

    struct Rig {
        engine: Engine<Fake>,
        used: Rc<Cell<f64>>,
        mode: Rc<Cell<u8>>,
        calls: Rc<Cell<u32>>,
    }

    fn rig() -> Rig {
        let used = Rc::new(Cell::new(0.0));
        let mode = Rc::new(Cell::new(0));
        let calls = Rc::new(Cell::new(0));
        let src = Fake {
            used: used.clone(),
            mode: mode.clone(),
            calls: calls.clone(),
        };
        Rig {
            engine: Engine::new(src, Settings::default(), Ledger::new()),
            used,
            mode,
            calls,
        }
    }

    #[test]
    fn first_tick_shows_label_and_green_tone() {
        let mut r = rig();
        r.used.set(349.90);
        let u = r.engine.tick(d(2026, 11, 12));
        assert_eq!(u.tray.label, "0 / 127");
        assert_eq!(u.tray.tone, Tone::Green);
        assert!(!u.notify);
        assert_eq!(u.snapshot.calendar.len(), 19); // 12 Nov .. 30 Nov
        assert_eq!(u.snapshot.trend.len(), 1);
    }

    #[test]
    fn label_follows_usage_and_colour_follows_pace() {
        let mut r = rig();
        r.used.set(0.0);
        r.engine.tick(d(2026, 11, 12)); // budget 2000/13 = 153.8
        r.used.set(170.0);
        assert_eq!(r.engine.tick(d(2026, 11, 12)).tray.tone, Tone::Amber); // 110%
        r.used.set(200.0);
        let u = r.engine.tick(d(2026, 11, 12));
        assert_eq!(u.tray.tone, Tone::Red); // 130%
        assert_eq!(u.tray.label, "200 / 154");
    }

    #[test]
    fn notifies_once_per_day_when_first_over_budget() {
        let mut r = rig();
        r.engine.tick(d(2026, 11, 12));
        r.used.set(160.0); // over 153.8
        assert!(r.engine.tick(d(2026, 11, 12)).notify);
        r.used.set(190.0);
        assert!(!r.engine.tick(d(2026, 11, 12)).notify); // already told today
                                                         // A new day over budget again notifies again.
        r.used.set(190.0);
        r.engine.tick(d(2026, 11, 13));
        r.used.set(190.0 + 200.0);
        assert!(r.engine.tick(d(2026, 11, 13)).notify);
    }

    #[test]
    fn no_notification_while_under_budget() {
        let mut r = rig();
        r.engine.tick(d(2026, 11, 12));
        r.used.set(100.0);
        assert!(!r.engine.tick(d(2026, 11, 12)).notify);
    }

    #[test]
    fn a_missing_cli_greys_the_tray_and_keeps_the_last_numbers() {
        let mut r = rig();
        r.used.set(50.0);
        r.engine.tick(d(2026, 11, 12));
        r.mode.set(1);
        let u = r.engine.tick(d(2026, 11, 12));
        assert_eq!(u.tray.tone, Tone::Grey);
        assert!(matches!(u.snapshot.notice, Some(Notice::Unavailable(_))));
        assert!(u.snapshot.view.is_some(), "last reading stays visible");
        assert!(!u.notify, "stale data must not raise alerts");
        // Recovers by itself.
        r.mode.set(0);
        let u = r.engine.tick(d(2026, 11, 12));
        assert!(u.snapshot.notice.is_none());
        assert_eq!(u.tray.tone, Tone::Green);
    }

    #[test]
    fn a_surprise_halts_and_stops_calling_the_cli_until_resume() {
        let mut r = rig();
        r.mode.set(2);
        let u = r.engine.tick(d(2026, 11, 12));
        assert!(matches!(u.snapshot.notice, Some(Notice::Halted(_))));
        assert_eq!(u.tray.tone, Tone::Grey);
        assert_eq!(r.calls.get(), 1);
        for _ in 0..4 {
            r.engine.tick(d(2026, 11, 12));
        }
        assert_eq!(r.calls.get(), 1, "no CLI calls while halted");
        r.mode.set(0);
        r.engine.resume();
        let u = r.engine.tick(d(2026, 11, 12));
        assert!(u.snapshot.notice.is_none());
        assert_eq!(r.calls.get(), 2);
    }

    #[test]
    fn manual_entry_works_even_with_no_reading_ever() {
        let mut r = rig();
        r.mode.set(2);
        r.engine.tick(d(2026, 11, 12)); // halted
        let u = r.engine.apply_manual(
            d(2026, 11, 12),
            ManualEntry {
                reset_on: d(2026, 12, 1),
                allowance: 2000.0,
                remaining: 1650.0,
            },
        );
        let v = u.snapshot.view.unwrap();
        assert!((v.daily_budget - 1650.0 / 13.0).abs() < 1e-9);
        assert!(u.snapshot.manual);
        assert!(
            matches!(u.snapshot.notice, Some(Notice::Halted(_))),
            "still halted, honestly shown"
        );
        assert!(!u.notify, "manual numbers do not raise alerts");
    }

    #[test]
    fn manual_entry_is_clamped_to_sane_values() {
        let mut r = rig();
        let u = r.engine.apply_manual(
            d(2026, 11, 12),
            ManualEntry {
                reset_on: d(2026, 12, 1),
                allowance: 100.0,
                remaining: 5000.0,
            },
        );
        assert_eq!(u.snapshot.view.unwrap().remaining, 100.0);
    }

    #[test]
    fn a_live_reading_replaces_manual_entry() {
        let mut r = rig();
        r.engine.apply_manual(
            d(2026, 11, 12),
            ManualEntry {
                reset_on: d(2026, 12, 1),
                allowance: 2000.0,
                remaining: 1000.0,
            },
        );
        r.used.set(500.0);
        let u = r.engine.tick(d(2026, 11, 12));
        assert!(!u.snapshot.manual);
        assert_eq!(u.snapshot.view.unwrap().remaining, 1500.0);
    }

    #[test]
    fn marking_leave_updates_budget_and_calendar_without_polling() {
        let mut r = rig();
        r.engine.tick(d(2026, 11, 12));
        let before = r
            .engine
            .update(d(2026, 11, 12))
            .snapshot
            .view
            .unwrap()
            .daily_budget;
        let calls = r.calls.get();
        let mut s = r.engine.settings().schedule.clone();
        s.leave_days.insert(d(2026, 11, 13));
        let u = r.engine.set_schedule(d(2026, 11, 12), s, true);
        let after = u.snapshot.view.unwrap().daily_budget;
        assert!(after > before);
        assert_eq!(
            r.calls.get(),
            calls,
            "editing the calendar must not run the CLI"
        );
        let fri = u
            .snapshot
            .calendar
            .iter()
            .find(|c| c.date == d(2026, 11, 13))
            .unwrap();
        assert!(fri.leave && !fri.working);
        assert!(u.snapshot.settings.setup_done);
    }

    #[test]
    fn with_no_reading_the_tray_says_so() {
        let mut r = rig();
        r.mode.set(1);
        let u = r.engine.tick(d(2026, 11, 12));
        assert_eq!(u.tray.label, "-- / --");
        assert_eq!(u.tray.tone, Tone::Grey);
        assert!(u.snapshot.view.is_none());
        assert!(u.snapshot.calendar.is_empty());
    }

    #[test]
    fn failed_poll_across_midnight_does_not_create_a_daily_record() {
        let mut r = rig();
        r.engine.tick(d(2026, 11, 12));
        let checked = r.engine.ledger().last_checked_at;
        r.mode.set(1);
        let stale = r.engine.tick(d(2026, 11, 13));
        assert_eq!(stale.snapshot.last_checked_at, checked);
        assert!(!r.engine.ledger().has_day(d(2026, 11, 13)));
        r.used.set(20.0);
        r.mode.set(0);
        let recovered = r.engine.tick(d(2026, 11, 13));
        assert_eq!(recovered.snapshot.view.unwrap().unallocated_usage, 20.0);
    }

    #[test]
    fn settings_tolerate_missing_fields_in_old_files() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
        let s: Settings = serde_json::from_str(r#"{"setup_done":true}"#).unwrap();
        assert!(s.setup_done);
    }
}
