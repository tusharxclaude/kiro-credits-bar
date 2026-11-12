# Contributing

Thanks for helping. This is a small project; the bar for a good contribution is a clear problem and
a test.

## The rule that matters most: never spend the user's credits

The app polls Kiro every five minutes. If its data source ever sent a prompt to the AI model, it
would burn credits on every poll, which is the opposite of what the app is for. So:

- Keep `--v2` pinned in `core/src/source.rs` (`USAGE_ARGS`). Read
  [ADR 0001](docs/adr/0001-read-usage-through-kiro-cli.md) before changing it.
- Anything unexpected (unparseable output, a timeout, a failed exit) must **halt** polling, not retry.
  `core/src/source.rs` has tests for this; please keep them passing.
- Don't add network calls, telemetry or any use of Kiro's login tokens.

## Setting up

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI runs the same checks on Linux, macOS and Windows. You don't need the Kiro CLI to develop: the
core is tested against fixtures, and `KCB_CLI` can point the app at a stand-in script.

## What would help most

- **`/usage` outputs from other plans.** Only Free and Pro+ have been seen. If yours differs (for
  example with overages enabled, or a Pro or Power plan), open an issue with the output or add a
  file under `tests/fixtures/`. **Remove anything personal first** (names, emails, account IDs).
- **Testing on Windows and Linux**, especially the tray icon, panel placement and notifications.
- **Bank holidays** by country or region. This needs a decision on where the data comes from. Open
  an issue to discuss it first.
- A V3-compatible data source, if you find a supported way to read usage without the model.

## Pull requests

- One change per pull request, with a test where the change affects behaviour.
- Use the project's words (see [GLOSSARY.md](GLOSSARY.md)): *daily budget*, *billing cycle*,
  *opening balance*, and so on. If you introduce a new concept, add it to the glossary.
- Decisions that are hard to reverse and surprising without context get a short note in
  `docs/adr/`.
- Keep the panel free of business logic. Budgets, pace and tray state are decided in `core/`.

By contributing you agree your work is licensed under the project's terms (MIT or Apache-2.0, at
your option).
