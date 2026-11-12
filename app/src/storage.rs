//! Settings and ledger files in the app data directory. Local only.

use chrono::{Duration, Local};
use kcb_core::engine::Settings;
use kcb_core::ledger::Ledger;
use std::fs;
use std::path::Path;

const SETTINGS: &str = "settings.json";
const LEDGER: &str = "ledger.json";
/// Keep roughly four billing cycles of history.
const KEEP_DAYS: i64 = 125;

/// Write via a temp file and rename so a crash never leaves half a file.
fn write_atomic(path: &Path, contents: &str) {
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, contents).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

/// A corrupt or missing file is not fatal: start fresh and keep a copy of the bad file.
fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn quarantine(path: &Path) {
    let _ = fs::rename(path, path.with_extension("corrupt"));
}

pub fn load(dir: &Path) -> (Settings, Ledger) {
    let _ = fs::create_dir_all(dir);
    let settings_path = dir.join(SETTINGS);
    let settings = match read(&settings_path) {
        Some(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
            quarantine(&settings_path);
            Settings::default()
        }),
        None => Settings::default(),
    };
    let ledger_path = dir.join(LEDGER);
    let ledger = match read(&ledger_path) {
        Some(text) => Ledger::from_json(&text).unwrap_or_else(|_| {
            quarantine(&ledger_path);
            Ledger::new()
        }),
        None => Ledger::new(),
    };
    (settings, ledger)
}

pub fn save(dir: &Path, settings: &Settings, ledger: &Ledger) {
    let _ = fs::create_dir_all(dir);
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        write_atomic(&dir.join(SETTINGS), &text);
    }
    let mut pruned = ledger.clone();
    pruned.prune_before(Local::now().date_naive() - Duration::days(KEEP_DAYS));
    write_atomic(&dir.join(LEDGER), &pruned.to_json());
}
