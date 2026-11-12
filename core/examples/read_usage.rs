//! Manual end-to-end check against the real Kiro CLI:
//!
//!     cargo run -p kcb-core --example read_usage [path-to-kiro-cli]
//!
//! Runs `kiro-cli chat --v2 --no-interactive /usage` once and prints today's view.
//! It reads and writes nothing on disk.

use chrono::Local;
use kcb_core::budget::Schedule;
use kcb_core::ledger::Ledger;
use kcb_core::source::{CliSource, PollOutcome, Poller};

fn main() {
    let program = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "kiro-cli".to_string());
    let mut poller = Poller::new(CliSource::new(program));
    match poller.poll() {
        PollOutcome::Reading(reading) => {
            let today = Local::now().date_naive();
            let view = Ledger::new().record(today, &reading, &Schedule::default());
            println!("{view:#?}");
        }
        PollOutcome::Unavailable(why) => eprintln!("kiro-cli not available: {why}"),
        PollOutcome::Halted(why) => eprintln!("halted (no retry): {why}"),
    }
}
