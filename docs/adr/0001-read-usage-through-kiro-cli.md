---
status: accepted
---

# Read usage by running `kiro-cli /usage`

Credits are metered on Kiro's servers, so the numbers are the same whichever client spent them. The app gets them by running `kiro-cli chat --no-interactive "/usage"` and parsing its text, which prints the plan, credits used and total, and the reset date. It never reads the IDE's cached login token and never calls Kiro's undocumented usage API.

## Considered Options

- **Call Kiro's undocumented `GetUsageLimits` API with the IDE's cached token.** Rejected: the endpoint is unsupported and could change, and the app would be using a credential it did not create.
- **Read the CLI's local session files.** Rejected: they only cover CLI sessions on one machine, so IDE usage would be invisible.
- **Manual entry only.** Kept as the fallback when the CLI is missing, logged out or its output cannot be parsed, not as the primary source.

## Consequences

The CLI must be installed and logged in, as any account. The parser depends on undocumented text and may break when Kiro changes the output. Captured so far (see `tests/fixtures/`): Free, and Pro+ managed by an organization. Both share the same first two lines (plan and reset date; credits used, total and percentage). The remaining lines differ by plan (`Overages: Disabled`, an admin-managed notice, a plan link), so the parser must rely on the first two lines and ignore the rest. The format with overages enabled has not been seen.

**The command must never reach the model.** In CLI 3.2.1, `chat --v2 --no-interactive "/usage"` (also the default fallback there) handles `/usage` locally and is free. `--v3` sends it to the AI model as a prompt, which ran tools and cost 0.87 credits in one test; another install reportedly defaults to v3 and answers "run Kiro CLI and type /usage". A poller that hits that path would spend credits on every poll. The app therefore pins `--v2`, validates that the output has the expected shape, and on any mismatch stops polling and falls back to manual entry rather than retrying. If Kiro removes v2, this decision must be revisited.
