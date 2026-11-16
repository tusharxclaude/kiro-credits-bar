"use strict";

// The panel is a pure view of the Snapshot the Rust side sends. It holds no business logic:
// budgets, pace and tray state are all decided in kcb-core.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const $ = (id) => document.getElementById(id);

let snap = null;

const fmt = (n) => Math.round(n).toLocaleString();
const fmt1 = (n) => n.toLocaleString(undefined, { maximumFractionDigits: 1 });
const parseDate = (s) => new Date(s + "T00:00:00");
const dayLabel = (s) => parseDate(s).toLocaleDateString(undefined, { day: "numeric", month: "short" });

function toneOf(view, notice) {
  if (notice) return "grey";
  if (!view) return "grey";
  return { OnTrack: "green", Ahead: "amber", WellAhead: "red" }[view.pace] || "grey";
}

function el(tag, attrs = {}, text) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v);
  }
  if (text !== undefined) e.textContent = text;
  return e;
}

function renderBanner(s) {
  const b = $("banner");
  b.className = "banner";
  b.replaceChildren();
  b.hidden = true;
  if (s.notice) {
    b.hidden = false;
    if (s.notice.kind === "Halted") {
      b.classList.add("halted");
      b.append(
        el("strong", {}, "Stopped reading Kiro. "),
        el("span", {}, "To protect your credits, the app will not retry on its own. "),
        el("div", { class: "muted" }, s.notice.reason),
        el("button", { onclick: () => run(invoke("resume")) }, "Resume")
      );
    } else {
      b.classList.add("unavailable");
      b.append(
        el("strong", {}, "Kiro CLI not found or not signed in. "),
        el("span", {}, "Showing the last reading; it retries every 5 minutes."),
        el("div", { class: "muted" }, s.notice.reason),
        el("button", { onclick: () => run(invoke("refresh")) }, "Retry now")
      );
    }
  } else if (s.manual) {
    b.hidden = false;
    b.classList.add("manual");
    b.append(el("span", {}, "These numbers were typed in by hand. A live Kiro reading replaces them."));
  }
}

function renderHero(s) {
  const v = s.view;
  const tone = toneOf(v, s.notice);
  const breakdown = $("budget-breakdown");
  breakdown.replaceChildren();
  $("coverage").textContent = "";
  if (v && Number.isFinite(v.base_allowance)) {
    for (const [label, value] of [["Base allowance", v.base_allowance], ["Carryover adjustment", v.carryover], ["Available today", v.daily_budget]]) {
      breakdown.append(el("dt", {}, label), el("dd", {}, (label === "Carryover adjustment" && value > 0.000001 ? "+" : "") + fmt1(Math.abs(value) < 0.000001 ? 0 : value)));
    }
    const since = v.first_checked_at ? new Date(v.first_checked_at).toLocaleTimeString([], {hour: "2-digit", minute: "2-digit"}) : null;
    $("coverage").textContent = since ? `Today tracked since ${since}.` : "Today's usage starts at the first reading, not midnight.";
    if (v.unallocated_usage > 0) $("coverage").textContent += ` ${fmt1(v.unallocated_usage)} credits used between recorded days are included in your balance and carryover; their daily breakdown is unknown.`;
  }
  $("hero").className = "card hero tone-" + tone;
  if (!v) {
    $("hero-label").textContent = "-- / --";
    $("pace").textContent = "No reading";
    $("bar-fill").style.width = "0";
    $("bar").setAttribute("aria-valuenow", "0");
    $("hero-foot").textContent = "Waiting for Kiro, or enter your credits by hand in Settings.";
    return;
  }
  $("hero-label").textContent = `${fmt(v.used_today)} / ${fmt(v.daily_budget)}`;
  // The track spans 0–140% of budget so the 100% mark and the red zone are both visible.
  const ratio = v.daily_budget > 0 ? v.used_today / v.daily_budget : v.used_today > 0 ? 1.4 : 0;
  $("bar-fill").style.width = Math.min(ratio / 1.4, 1) * 100 + "%";
  $("bar").setAttribute("aria-valuenow", String(Math.min(140, Math.round(ratio * 100))));

  let pace, foot;
  const isDayOff = v.daily_budget === 0 && v.working_days_left >= 0 && !isWorkingToday(s);
  if (isDayOff) {
    pace = v.used_today > 0 ? "Day off — used" : "Day off";
    foot = v.used_today > 0
      ? `${fmt(v.used_today)} credits used on a day off. It reduces the next working day's carryover.`
      : "Nothing planned today. Unused credits stay in the pot.";
  } else if (v.budget_left >= 0) {
    pace = tone === "green" ? "On track" : "Within budget";
    foot = `${fmt(v.budget_left)} left of today's budget.`;
  } else {
    pace = tone === "red" ? "Well over budget" : "Over budget";
    foot = `${fmt(-v.budget_left)} over today's budget. It will be deducted from the next working day.`;
  }
  if (s.notice) pace = "Last reading";
  $("pace").textContent = pace;
  $("hero-foot").textContent = foot;
}

function isWorkingToday(s) {
  const d = s.calendar.find((c) => c.date === s.today);
  return d ? d.working : false;
}

function renderStats(s) {
  const v = s.view;
  const box = $("stats");
  box.replaceChildren();
  if (!v) return;
  const pct = v.allowance > 0 ? ((v.allowance - v.remaining) / v.allowance) * 100 : 0;
  for (const [value, label] of [
    [fmt(v.remaining), "credits left"],
    [String(v.working_days_left), "working days left"],
    [fmt1(pct) + "%", "of allowance used"],
  ]) {
    const d = el("div", { class: "stat" });
    d.append(el("b", {}, value), el("span", {}, label));
    box.append(d);
  }
}

function renderCycle(s) {
  const v = s.view;
  $("cycle").textContent = v
    ? `Resets ${dayLabel(v.reset_on)} · ${v.working_days_left} working day${v.working_days_left === 1 ? "" : "s"} left`
    : "Waiting for the first reading…";
  const plan = $("plan");
  plan.hidden = !v;
  if (v) plan.textContent = v.plan;
}

function renderTrend(s) {
  const box = $("trend");
  box.replaceChildren();
  const pts = s.trend;
  if (!pts.length) {
    box.append(el("div", { class: "empty" }, "The chart fills in as the app records each day."));
    return;
  }
  const W = 330, H = 120, padL = 4, padR = 4, padT = 8, padB = 18;
  const maxV = Math.max(10, ...pts.map((p) => Math.max(p.used, p.budget))) * 1.1;
  const slot = (W - padL - padR) / Math.max(pts.length, 7);
  const y = (val) => H - padB - (val / maxV) * (H - padT - padB);
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", "Credits used each day compared with the daily budget");
  const add = (tag, attrs, text) => {
    const e = document.createElementNS(ns, tag);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
    if (text) e.textContent = text;
    svg.append(e);
    return e;
  };
  add("line", { x1: padL, x2: W - padR, y1: H - padB, y2: H - padB, stroke: "currentColor", "stroke-opacity": ".25" });
  let path = "";
  pts.forEach((p, i) => {
    const x = padL + i * slot;
    const bw = Math.max(4, slot * 0.6);
    const top = y(p.used);
    const over = p.budget > 0 ? p.used > p.budget : p.used > 0;
    const bar = add("rect", {
      x: x + (slot - bw) / 2, y: top, width: bw, height: Math.max(0, H - padB - top), rx: 2,
      fill: over ? "var(--red)" : "var(--brand)", "fill-opacity": ".85",
    });
    const t = document.createElementNS(ns, "title");
    t.textContent = `${dayLabel(p.date)}: used ${fmt(p.used)}, budget ${fmt(p.budget)}`;
    bar.append(t);
    const cx = x + slot / 2;
    if (pts.length === 1) {
      add("circle", { cx, cy: y(p.budget), r: 2.5, fill: "currentColor", "fill-opacity": ".6" });
    }
    path += `${i === 0 ? "M" : "L"}${cx.toFixed(1)},${y(p.budget).toFixed(1)} `;
    if (pts.length <= 12 || i % 2 === 0) {
      add("text", { x: cx, y: H - 4, "text-anchor": "middle", "font-size": "9", fill: "currentColor", "fill-opacity": ".6" },
        String(parseDate(p.date).getDate()));
    }
  });
  add("path", { d: path, fill: "none", stroke: "currentColor", "stroke-opacity": ".6", "stroke-width": "1.6", "stroke-dasharray": "3 2" });
  box.append(svg);
}

function renderCalendar(s) {
  const head = $("cal-head");
  const grid = $("calendar");
  head.replaceChildren(...WEEKDAYS.map((d) => el("span", {}, d[0])));
  grid.replaceChildren();
  if (!s.calendar.length) {
    grid.append(el("div", { class: "muted" }, "Shown once Kiro reports when the cycle resets."));
    grid.style.display = "block";
    return;
  }
  grid.style.display = "";
  for (let i = 0; i < s.calendar[0].weekday; i++) grid.append(el("span", { class: "day blank" }, "."));
  const workingWeekdays = new Set(s.settings.schedule.working_weekdays);
  for (const c of s.calendar) {
    const day = parseDate(c.date).getDate();
    const togglable = workingWeekdays.has(c.weekday);
    const cls = ["day", c.working ? "work" : "off", c.leave ? "leave" : "", c.date === s.today ? "today" : ""].join(" ");
    const label = `${dayLabel(c.date)}: ${c.leave ? "leave" : c.working ? "working day" : "day off"}`;
    if (togglable) {
      grid.append(el("button", { class: cls, title: label, "aria-label": label, "aria-pressed": String(c.leave), onclick: () => toggleLeave(c.date) }, String(day)));
    } else {
      grid.append(el("span", { class: cls, title: label, "aria-label": label }, String(day)));
    }
  }
}

function weekdayBoxes(container, s, onchange) {
  container.replaceChildren();
  const on = new Set(s.settings.schedule.working_weekdays);
  WEEKDAYS.forEach((name, i) => {
    const input = el("input", { type: "checkbox" });
    input.checked = on.has(i);
    input.addEventListener("change", onchange);
    input.dataset.day = String(i);
    const label = el("label");
    label.append(input, document.createTextNode(name));
    container.append(label);
  });
}

function pickedWeekdays(container) {
  return [...container.querySelectorAll("input")].filter((i) => i.checked).map((i) => Number(i.dataset.day));
}

function saveSchedule(weekdays, leaveDays, setupDone) {
  return run(invoke("save_schedule", { weekdays, leaveDays, setupDone }));
}

function toggleLeave(date) {
  const leave = new Set(snap.settings.schedule.leave_days);
  leave.has(date) ? leave.delete(date) : leave.add(date);
  saveSchedule(snap.settings.schedule.working_weekdays, [...leave], snap.settings.setup_done);
}

function renderSettings(s) {
  const setup = $("setup");
  setup.hidden = s.settings.setup_done;
  if (!s.settings.setup_done) {
    weekdayBoxes($("setup-weekdays"), s, () => {});
  }
  // Preserve keyboard focus while a working-day checkbox is in use.
  if (!$("weekdays").contains(document.activeElement)) {
    weekdayBoxes($("weekdays"), s, () =>
      saveSchedule(pickedWeekdays($("weekdays")), s.settings.schedule.leave_days, true));
  }
  if (document.activeElement !== $("cli-path")) $("cli-path").value = s.settings.cli_path || "";
  const v = s.view;
  if (v && !$("manual").open) {
    $("m-allow").value = v.allowance;
    $("m-remaining").value = Math.round(v.remaining * 100) / 100;
    $("m-reset").value = v.reset_on;
  }
}

function render(s) {
  snap = s;
  renderCycle(s);
  renderBanner(s);
  renderHero(s);
  renderStats(s);
  renderTrend(s);
  renderCalendar(s);
  renderSettings(s);
  renderFreshness();
}

function renderFreshness() {
  const at = snap?.last_checked_at;
  $("updated").textContent = snap?.manual ? "Manual reading" : at
    ? "Last checked " + new Date(at).toLocaleString([], {month: "short", day: "numeric", hour: "2-digit", minute: "2-digit"})
    : "No successful check yet";
}

const autostart = $("autostart");
invoke("autostart_enabled").then((enabled) => {
  autostart.checked = enabled;
  autostart.disabled = false;
  $("autostart-status").textContent = "You can change this anytime.";
}).catch((e) => { $("autostart-status").textContent = "Login setting unavailable: " + e; });
autostart.addEventListener("change", async () => {
  const wanted = autostart.checked;
  autostart.disabled = true;
  try {
    autostart.checked = await invoke("set_autostart", {enabled: wanted});
    $("autostart-status").textContent = autostart.checked ? "Will start at your next login." : "Automatic launch is off.";
  } catch (e) {
    autostart.checked = !wanted;
    $("autostart-status").textContent = "Could not change login setting: " + e;
  } finally { autostart.disabled = false; }
});

async function run(promise) {
  const btn = $("btn-refresh");
  btn.classList.add("spin");
  try {
    render(await promise);
  } catch (e) {
    $("banner").hidden = false;
    $("banner").className = "banner halted";
    $("banner").textContent = "Something went wrong: " + e;
  } finally {
    btn.classList.remove("spin");
  }
}

$("btn-refresh").addEventListener("click", () => run(invoke("refresh")));
$("btn-close").addEventListener("click", () => invoke("hide_panel"));
$("btn-quit").addEventListener("click", () => invoke("quit_app"));
$("btn-setup-done").addEventListener("click", () =>
  saveSchedule(pickedWeekdays($("setup-weekdays")), snap.settings.schedule.leave_days, true));
$("btn-cli-save").addEventListener("click", () => run(invoke("set_cli_path", { path: $("cli-path").value })));
$("btn-manual").addEventListener("click", () => {
  const allowance = parseFloat($("m-allow").value);
  const remaining = parseFloat($("m-remaining").value);
  const resetOn = $("m-reset").value;
  if (!(allowance > 0) || !(remaining >= 0) || !resetOn) {
    alert("Please fill in the allowance, the credits remaining and the reset date.");
    return;
  }
  run(invoke("manual_entry", { entry: { reset_on: resetOn, allowance, remaining } }));
});

// Keep navigation independent of live snapshots, so polling never changes the page.
const tabs = [...document.querySelectorAll('[role="tab"]')];
function selectPage(tab, focus = false) {
  for (const item of tabs) {
    const selected = item === tab;
    item.setAttribute("aria-selected", String(selected));
    item.tabIndex = selected ? 0 : -1;
    $("page-" + item.dataset.page).hidden = !selected;
  }
  if (focus) tab.focus();
}
tabs.forEach((tab, index) => {
  tab.addEventListener("click", () => selectPage(tab));
  tab.addEventListener("keydown", (event) => {
    let next;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    if (event.key === "ArrowLeft") next = (index + tabs.length - 1) % tabs.length;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = tabs.length - 1;
    if (next === undefined) return;
    event.preventDefault();
    selectPage(tabs[next], true);
  });
});

// Live updates pushed from the Rust poller.
tauri.event.listen("state", (e) => render(e.payload));
invoke("get_state").then(render);
