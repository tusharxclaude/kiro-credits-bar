# Kiro Credits Bar

[![CI](https://github.com/tusharxclaude/kiro-credits-bar/actions/workflows/ci.yml/badge.svg)](https://github.com/tusharxclaude/kiro-credits-bar/actions/workflows/ci.yml)

A small tray app that tells you **how many Kiro credits to spend today**.

Kiro gives you a pot of credits per billing cycle and nothing to help you pace it. Kiro Credits Bar
spreads what is left across the working days left, shows today's budget as a bar in your menu bar,
and tells you whether you are ahead of or behind pace.

> **Unofficial.** This project is not affiliated with, endorsed by or sponsored by Amazon Web
> Services or the Kiro team. "Kiro" is a trademark of its owners and is used here only to say what
> the tool is for. See [Branding](#branding).

It is **read-only**: it never limits, blocks or changes anything in Kiro.

## What you get

- A **tray icon** that changes colour with your pace, with `used / budget` next to it on macOS
  (for example `142 / 180`): green up to 100% of today's budget, amber up to 120%, red above.
- One **desktop notification** the first time you go over budget each day.
- A **panel** with the bar, credits left, working days left, a **usage trend** chart against your
  daily budget, and a calendar where you tap a day to mark leave.
- A **working week you choose** (Monday to Friday by default).
- A **manual entry** form for when Kiro can't be read.

### How the budget works

```
base allowance = credits at the first tracked balance ÷ working days from that day to reset
available today = base allowance + accumulated carryover (bounded by 0 and credits left)
```

- The base allowance is anchored to the earliest recorded opening balance in the cycle. Changing your schedule recalculates it; usage during the day does not.
- Unused allowance carries into the next working day; overspending is deducted. With a base of 100, spending 80 gives you 120 next time; spending 120 leaves 80. Carryover clears at the next billing cycle.
- Today shows the base allowance, carryover adjustment, and available budget separately. A negative adjustment can reduce the available budget to zero.
- A day off (a non-working weekday, or leave you marked) has a budget of 0. Credits spent on one
  are not an error; they reduce the next working day's carryover.
- Days you didn't use Kiro need no special handling. The unused allowance carries into the
  next working day.
- The cycle length and allowance come straight from Kiro, so nothing needs configuring.

## Requirements

- The **[Kiro CLI](https://kiro.dev)** (`kiro-cli`), installed and signed in. It can be signed in
  with any account; the app reports whichever account the CLI uses. Credits are tracked on Kiro's
  servers, so usage from the IDE shows up too.
- [Rust](https://rustup.rs) (stable) to build from source.
- **Linux only:** the Tauri system libraries, for example on Debian or Ubuntu:
  `sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential`

## Install and run

Download preview installers for macOS, Windows and Linux from the
[Releases page](https://github.com/tusharxclaude/kiro-credits-bar/releases). Preview builds are
**unsigned**, so your OS will warn you the first time you open the app; see
[docs/RELEASING.md](docs/RELEASING.md) for what to expect and why. From a checkout:

```sh
cargo run -p kcb-app            # debug build
cargo build -p kcb-app --release
```

On first launch the panel opens so you can confirm your working week. After that the app lives in
the tray. Click the icon to open the panel, right-click for **Refresh now** and **Quit**.

| Platform | Status |
| --- | --- |
| macOS | Built and run by the author. |
| Windows, Linux | Expected to work but **not yet tested** on real desktops. CI builds them. |

### Settings

The panel has four icon tabs: **Today**, **History**, **Schedule**, and **Settings**.
Choose working days and leave in Schedule. Settings includes an optional **Start at login** toggle
(off until enabled), the location of `kiro-cli`, and manual entry. Login registration uses the OS
and reports failures in the panel; enable it from your installed app so it points to a stable location.

If the CLI isn't found automatically, set its path in Settings. The app looks on your `PATH`, then in `~/.local/bin`,
`/usr/local/bin` and `/opt/homebrew/bin`.

For development there are two environment variables:

| Variable | Effect |
| --- | --- |
| `KCB_CLI` | Use this program instead of looking for `kiro-cli`. |
| `KCB_DATA_DIR` | Keep settings and history in this folder instead of the app data directory. |

## Privacy

- The app makes **no network connections of its own**. It runs `kiro-cli` and reads its output.
- Settings and your daily history are stored in two small JSON files (`settings.json`,
  `ledger.json`) in the app data directory on your machine. Nothing is sent anywhere, and there is
  no telemetry.
- It does not read, copy or use Kiro's login tokens. `kiro-cli` handles its own sign-in.

## Known limits

- **It parses text that Kiro doesn't document.** The app runs `kiro-cli chat --v2 --no-interactive
  "/usage"` and reads the first two lines. So far only the Free and Pro+ formats have been seen.
  If the output ever looks different, the app stops and says so instead of guessing.
- **It stops on any surprise, on purpose.** On the CLI's newer *V3* engine the same command is
  sent to the AI model as a prompt and costs credits. So the app pins `--v2`, and if the output
  doesn't have the expected shape it **stops polling** and waits for you to press **Resume**, so it
  can never spend credits in a loop. See
  [ADR 0001](docs/adr/0001-read-usage-through-kiro-cli.md). If Kiro removes V2, a new data source
  will be needed.
- **Usage is Kiro's estimate** and refreshes about every five minutes, which is how often the app
  polls.
- **Daily coverage begins at the first reading, not midnight.** The panel shows that time and the
  last successful live check. Usage between recorded days reduces the balance and carryover, but is
  shown separately with an unknown daily breakdown. Days without readings are absent from History.
  The initial reading cannot reconstruct usage earlier that day. Failed checks do not create records
  for new days or advance the last-check time.
- **Bank holidays aren't built in yet.** Mark them as leave for now.

## Development

```sh
cargo test --workspace                      # no Kiro CLI needed
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

| Path | What it is |
| --- | --- |
| `core/` | `kcb-core`: pure logic with no UI. Parsing, budget, ledger, engine, CLI runner. |
| `app/` | `kcb-app`: the Tauri tray app. The panel is plain HTML, CSS and JS in `app/ui/`. |
| `tests/fixtures/` | Real `/usage` outputs the parser is tested against. |
| `docs/adr/` | Decision records: why it reads the CLI, and why Tauri. |
| `GLOSSARY.md` | The project's vocabulary (credit, billing cycle, daily budget, pace…). |
| `tools/make_icons.py` | Renders the app and tray icons. |

To work on the panel without Tauri, open `app/ui/index.html?state=ok` in a browser. Other mock
states are `over`, `halted`, `unavailable`, `setup`, `dayoff`, `manual` and `nodata`. To run the app
without the real CLI, point `KCB_CLI` at any script that prints a `/usage` output.

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

## Branding

The name uses "Kiro" descriptively. The colours follow kiro.dev's palette. The icons are original
artwork, and no Kiro logo or mascot is included. See [`app/icons/README.md`](app/icons/README.md).

## Licence

Dual-licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT licence](LICENSE-MIT)

at your option. Unless you say otherwise, any contribution you submit for inclusion is licensed as
above, without extra terms.
