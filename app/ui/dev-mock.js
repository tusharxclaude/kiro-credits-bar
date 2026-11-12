"use strict";

// Development aid: lets the panel be opened in a plain browser (no Tauri) with sample data.
// Inside the real app `window.__TAURI__` exists and this file does nothing.
// Open index.html?state=halted|unavailable|over|dayoff|setup to preview other states.

(function () {
  if (window.__TAURI__) return;

  const params = new URLSearchParams(location.search);
  const mode = params.get("state") || "ok";
  const iso = (d) => d.toISOString().slice(0, 10);
  const addDays = (d, n) => new Date(d.getTime() + n * 86400000);

  const today = new Date("2026-11-12T12:00:00Z");
  const reset = new Date("2026-12-01T12:00:00Z");
  const leave = new Set(["2026-11-17"]);
  let weekdays = [0, 1, 2, 3, 4];
  let setupDone = mode !== "setup";

  function calendar() {
    const out = [];
    for (let d = today; d < reset; d = addDays(d, 1)) {
      const wd = (d.getUTCDay() + 6) % 7;
      const date = iso(d);
      out.push({ date, weekday: wd, working: weekdays.includes(wd) && !leave.has(date), leave: leave.has(date) });
    }
    return out;
  }

  function build() {
    const cal = calendar();
    const workingLeft = cal.filter((c) => c.working).length;
    const opening = 1650.1;
    const used = { over: 190, dayoff: 30 }[mode] ?? 63;
    const budget = mode === "dayoff" ? 0 : opening / Math.max(workingLeft, 1);
    const ratio = budget > 0 ? used / budget : used > 0 ? 9 : 0;
    const pace = ratio > 1.2 ? "WellAhead" : ratio > 1 ? "Ahead" : "OnTrack";
    const trend = [
      ["2026-11-02", 118, 100], ["2026-11-03", 82, 104], ["2026-11-04", 140, 103],
      ["2026-11-05", 95, 108], ["2026-11-06", 60, 107], ["2026-11-09", 120, 110],
      ["2026-11-10", 30, 114], ["2026-11-11", 90, 120], ["2026-11-12", used, budget],
    ].map(([date, u, b]) => ({ date, used: u, budget: b }));
    return {
      today: iso(today),
      view: mode === "nodata" ? null : {
        plan: "KIRO PRO+", allowance: 2000, reset_on: iso(reset), opening_balance: opening,
        remaining: opening - used, used_today: used, daily_budget: budget, budget_left: budget - used,
        working_days_left: workingLeft, pace,
      },
      notice: mode === "halted" ? { kind: "Halted", reason: "unexpected /usage output: first line is not `Estimated Usage | resets on <date> | <plan>`" }
        : mode === "unavailable" ? { kind: "Unavailable", reason: "No such file or directory (os error 2)" } : null,
      manual: mode === "manual",
      trend: mode === "nodata" ? [] : trend,
      calendar: mode === "nodata" ? [] : cal,
      settings: { schedule: { working_weekdays: weekdays, leave_days: [...leave] }, setup_done: setupDone, cli_path: null },
    };
  }

  const handlers = {
    get_state: () => build(),
    refresh: () => build(),
    resume: () => build(),
    set_cli_path: () => build(),
    manual_entry: () => build(),
    save_schedule: ({ weekdays: w, leaveDays, setupDone: s }) => {
      weekdays = w; leave.clear(); leaveDays.forEach((d) => leave.add(d)); setupDone = s;
      return build();
    },
    hide_panel: () => {},
    quit_app: () => {},
  };

  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => handlers[cmd](args || {}) },
    event: { listen: async () => () => {} },
  };
})();
