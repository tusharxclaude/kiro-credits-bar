// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod storage;

use chrono::{Local, NaiveDate};
use kcb_core::budget::Schedule;
use kcb_core::engine::{Engine, ManualEntry, Snapshot, Tone, TrayState, Update};
use kcb_core::source::CliSource;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

/// Kiro refreshes usage at most every 5 minutes, so polling faster gains nothing.
const POLL_EVERY: Duration = Duration::from_secs(300);
/// Ignore a tray click this soon after the panel hid itself on blur (that blur was the click).
const REOPEN_GUARD_MS: u64 = 400;

struct Shared {
    engine: Mutex<Engine<CliSource>>,
    latest: Mutex<Snapshot>,
    data_dir: PathBuf,
    last_hidden_ms: AtomicU64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Where `kiro-cli` lives. GUI apps on macOS do not inherit the shell PATH, so look in the usual places.
fn find_cli(explicit: &Option<String>) -> PathBuf {
    if let Some(p) = explicit.as_ref().filter(|p| !p.trim().is_empty()) {
        return PathBuf::from(p.trim());
    }
    if let Ok(p) = std::env::var("KCB_CLI") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) {
        "kiro-cli.exe"
    } else {
        "kiro-cli"
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        dirs.push(PathBuf::from(&home).join(".local").join("bin"));
    }
    dirs.extend(["/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from));
    for dir in dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(name) // let the spawn fail; the panel explains it
}

fn tone_icon(app: &AppHandle, tone: Tone) -> tauri::Result<Image<'static>> {
    let file = match tone {
        Tone::Green => "tray-green.png",
        Tone::Amber => "tray-amber.png",
        Tone::Red => "tray-red.png",
        Tone::Grey => "tray-grey.png",
    };
    // Icons are embedded at build time so the app has no loose files to lose.
    let bytes: &'static [u8] = match file {
        "tray-green.png" => include_bytes!("../icons/tray-green.png"),
        "tray-amber.png" => include_bytes!("../icons/tray-amber.png"),
        "tray-red.png" => include_bytes!("../icons/tray-red.png"),
        _ => include_bytes!("../icons/tray-grey.png"),
    };
    let _ = app;
    Image::from_bytes(bytes)
}

fn apply_tray(app: &AppHandle, tray: &TrayState) {
    if let Some(icon) = app.tray_by_id("main") {
        if let Ok(img) = tone_icon(app, tray.tone) {
            let _ = icon.set_icon(Some(img));
        }
        // The text next to the icon is macOS-only; elsewhere the tooltip carries it.
        let _ = icon.set_title(Some(tray.label.clone()));
        let _ = icon.set_tooltip(Some(format!("{}\n{}", tray.label, tray.tooltip)));
    }
}

/// Push an engine update to disk, the tray, the panel and (once a day) a notification.
fn publish(app: &AppHandle, shared: &Shared, update: Update) -> Snapshot {
    {
        let engine = shared.engine.lock().unwrap();
        storage::save(&shared.data_dir, engine.settings(), engine.ledger());
    }
    *shared.latest.lock().unwrap() = update.snapshot.clone();
    apply_tray(app, &update.tray);
    if update.notify {
        if let Some(v) = &update.snapshot.view {
            let _ = app
                .notification()
                .builder()
                .title("Kiro credits: over today's budget")
                .body(format!(
                    "Used {:.0} credits against a budget of {:.0}. Overspending carries into the next working day.",
                    v.used_today, v.daily_budget
                ))
                .show();
        }
    }
    let _ = app.emit("state", &update.snapshot);
    update.snapshot
}

/// Run a closure on the engine on a worker thread (polling can take up to 30 seconds).
async fn with_engine<F>(app: AppHandle, shared: Arc<Shared>, f: F) -> Snapshot
where
    F: FnOnce(&mut Engine<CliSource>) -> Update + Send + 'static,
{
    let s2 = shared.clone();
    let update = tauri::async_runtime::spawn_blocking(move || {
        let mut engine = s2.engine.lock().unwrap();
        let path = find_cli(&engine.settings().cli_path);
        engine.source_mut().program = path;
        f(&mut engine)
    })
    .await
    .expect("engine task panicked");
    publish(&app, &shared, update)
}

#[tauri::command]
fn get_state(shared: tauri::State<'_, Arc<Shared>>) -> Snapshot {
    shared.latest.lock().unwrap().clone()
}

#[tauri::command]
async fn refresh(
    app: AppHandle,
    shared: tauri::State<'_, Arc<Shared>>,
) -> Result<Snapshot, String> {
    Ok(with_engine(app, shared.inner().clone(), |e| e.tick(today())).await)
}

#[tauri::command]
async fn resume(app: AppHandle, shared: tauri::State<'_, Arc<Shared>>) -> Result<Snapshot, String> {
    Ok(with_engine(app, shared.inner().clone(), |e| {
        e.resume();
        e.tick(today())
    })
    .await)
}

#[tauri::command]
async fn save_schedule(
    app: AppHandle,
    shared: tauri::State<'_, Arc<Shared>>,
    weekdays: Vec<u8>,
    leave_days: Vec<NaiveDate>,
    setup_done: bool,
) -> Result<Snapshot, String> {
    let schedule = Schedule {
        working_weekdays: weekdays.into_iter().filter(|d| *d < 7).collect(),
        leave_days: leave_days.into_iter().collect(),
    };
    Ok(with_engine(app, shared.inner().clone(), move |e| {
        e.set_schedule(today(), schedule, setup_done)
    })
    .await)
}

#[tauri::command]
async fn manual_entry(
    app: AppHandle,
    shared: tauri::State<'_, Arc<Shared>>,
    entry: ManualEntry,
) -> Result<Snapshot, String> {
    Ok(with_engine(app, shared.inner().clone(), move |e| {
        e.apply_manual(today(), entry)
    })
    .await)
}

#[tauri::command]
async fn set_cli_path(
    app: AppHandle,
    shared: tauri::State<'_, Arc<Shared>>,
    path: Option<String>,
) -> Result<Snapshot, String> {
    Ok(with_engine(app, shared.inner().clone(), move |e| {
        e.set_cli_path(path.filter(|p| !p.trim().is_empty()));
        e.source_mut().program = find_cli(&e.settings().cli_path);
        e.resume();
        e.tick(today())
    })
    .await)
}

#[tauri::command]
fn autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())?;
    manager.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_panel(window: WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Open the panel next to the tray icon: below it on a top bar, above it on a bottom taskbar.
fn show_panel_near(window: &WebviewWindow, click: PhysicalPosition<f64>) {
    let size = window
        .outer_size()
        .unwrap_or(tauri::PhysicalSize::new(380, 640));
    let (w, h) = (size.width as f64, size.height as f64);
    let (mut x, mut y) = (click.x - w / 2.0, click.y + 12.0);
    if let Ok(Some(monitor)) = window
        .current_monitor()
        .or_else(|_| window.primary_monitor())
    {
        let mpos = monitor.position();
        let msize = monitor.size();
        let (left, top) = (mpos.x as f64, mpos.y as f64);
        let (right, bottom) = (left + msize.width as f64, top + msize.height as f64);
        if click.y > top + (bottom - top) / 2.0 {
            y = click.y - h - 12.0; // taskbar at the bottom
        }
        x = x.clamp(left + 8.0, (right - w - 8.0).max(left + 8.0));
        y = y.clamp(top + 8.0, (bottom - h - 8.0).max(top + 8.0));
    }
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.show();
    let _ = window.set_focus();
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_state,
            autostart_enabled,
            set_autostart,
            refresh,
            resume,
            save_schedule,
            manual_entry,
            set_cli_path,
            hide_panel,
            quit_app
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let data_dir = std::env::var_os("KCB_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.path().app_data_dir().expect("app data dir"));
            let (settings, ledger) = storage::load(&data_dir);
            let first_run = !settings.setup_done;
            let program = find_cli(&settings.cli_path);
            let mut engine = Engine::new(CliSource::new(program), settings, ledger);
            let initial = engine.update(today()); // no reading yet: an empty, grey state
            let shared = Arc::new(Shared {
                engine: Mutex::new(engine),
                latest: Mutex::new(initial.snapshot.clone()),
                data_dir,
                last_hidden_ms: AtomicU64::new(0),
            });
            app.manage(shared.clone());

            // Tray icon with a small menu; left click toggles the panel.
            let refresh_item =
                MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_item, &quit_item])?;
            TrayIconBuilder::with_id("main")
                .icon(tone_icon(app.handle(), Tone::Grey)?)
                .tooltip("Kiro credits")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event({
                    let shared = shared.clone();
                    move |app, event| match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "refresh" => {
                            let (app, shared) = (app.clone(), shared.clone());
                            tauri::async_runtime::spawn(async move {
                                with_engine(app.clone(), shared, |e| e.tick(today())).await;
                            });
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event({
                    let shared = shared.clone();
                    move |tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            position,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else if now_ms()
                                    .saturating_sub(shared.last_hidden_ms.load(Ordering::Relaxed))
                                    > REOPEN_GUARD_MS
                                {
                                    show_panel_near(&window, position);
                                }
                            }
                        }
                    }
                })
                .build(app)?;
            apply_tray(app.handle(), &initial.tray);

            // Hide the panel when it loses focus, remembering when.
            if let Some(window) = app.get_webview_window("main") {
                let (w, shared) = (window.clone(), shared.clone());
                window.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        shared.last_hidden_ms.store(now_ms(), Ordering::Relaxed);
                        let _ = w.hide();
                    }
                });
                if first_run {
                    // Nothing else to anchor to yet: greet the user near the top right.
                    let _ = window.center();
                    let _ = window.show();
                }
            }

            let _ = app.notification().request_permission();

            // Poll now, then every five minutes.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    with_engine(handle.clone(), shared.clone(), |e| e.tick(today())).await;
                    tokio_free_sleep(POLL_EVERY).await;
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Kiro Credits Bar");
}

/// Sleep without pulling in an async runtime crate of our own.
async fn tokio_free_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}
