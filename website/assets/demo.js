// Interactive demo of the Lantai desktop app for the homepage. It mirrors the real app's
// layout (sidebar calendar and lists, Report / Sessions tabs, outline reports with anchors,
// the session drawer) and runs entirely on the made-up data in demo-data.js.
(function () {
  "use strict";

  var DATA = window.LANTAI_DEMO_DATA;
  var root = document.getElementById("lantai-demo");
  if (!DATA || !root) return;

  /* ------------------------------------------------------------------ dates */

  var WD_SHORT = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  // The app's interface is English in every locale; only report text follows DATA.locale,
  // matching the real app, whose reports use the prompt language.
  var ZH = DATA.locale === "zh";
  var WD_LONG = ZH
    ? ["周日", "周一", "周二", "周三", "周四", "周五", "周六"]
    : ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  var WD_GRID = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
  var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

  function parse(d) {
    var p = d.split("-").map(Number);
    return new Date(Date.UTC(p[0], p[1] - 1, p[2]));
  }
  function fmt(dt) { return dt.toISOString().slice(0, 10); }
  function addDays(d, n) { var t = parse(d); t.setUTCDate(t.getUTCDate() + n); return fmt(t); }
  function weekday(d) { return parse(d).getUTCDay(); }
  function mondayOf(d) { return addDays(d, -((weekday(d) + 6) % 7)); }
  function weekKey(d) {
    var thu = parse(addDays(mondayOf(d), 3));
    var y = thu.getUTCFullYear();
    var wk = Math.floor((thu - Date.UTC(y, 0, 1)) / 86400000 / 7) + 1;
    return y + "-W" + (wk < 10 ? "0" : "") + wk;
  }
  function weekMonth(monday) { return addDays(monday, 3).slice(0, 7); }
  function pad(n) { return (n < 10 ? "0" : "") + n; }
  function hhmm(min) { return pad(Math.floor(min / 60) % 24) + ":" + pad(min % 60); }
  function fmtMinutes(m) {
    if (m < 60) return m + "m";
    var h = m / 60;
    return (h >= 10 ? Math.round(h) : h.toFixed(1).replace(/\.0$/, "")) + "h";
  }
  function hash(s) {
    var h = 2166136261;
    for (var i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
    return h >>> 0;
  }
  function esc(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }

  var TODAY = DATA.today;
  var CUR_WEEK = weekKey(TODAY);
  var CUR_MONTH = TODAY.slice(0, 7);
  var SHOWN_MONTH = CUR_MONTH; // The calendar is fixed to this month.
  var GRID_START = mondayOf(SHOWN_MONTH + "-01");

  /* ------------------------------------------------------------ projects */

  var projects = DATA.projects.map(function (p, i) {
    return { id: i + 1, key: p.key, name: p.name, desc: p.desc, dir: p.dir, remote: p.remote, agents: p.agents, files: p.files };
  });
  var projByKey = {};
  projects.forEach(function (p) { projByKey[p.key] = p; });
  function projById(id) { return projects.filter(function (p) { return p.id === id; })[0] || null; }

  var rules = [];
  projects.forEach(function (p, i) {
    rules.push({ id: rules.length + 1, projectId: p.id, kind: "Directory", value: p.dir, auto: true });
    rules.push({ id: rules.length + 1, projectId: p.id, kind: "Remote", value: p.remote, auto: i % 2 === 0 });
  });
  rules.push({ id: rules.length + 1, projectId: projByKey.C.id, kind: "Pattern", value: "(?i)benchmark|parser", auto: false });

  /* -------------------------------------------------------------- slices */

  var FOLLOW_UPS = ["Go ahead and make that change.", "Add a test that covers it.", "Anything else we should check?"];
  var slices = [];
  var miscIdx = 0;
  Object.keys(DATA.schedule).sort().forEach(function (date) {
    var cursor = 9 * 60 + (hash(date) % 50);
    DATA.schedule[date].forEach(function (code) {
      var id = slices.length + 1;
      var h = hash(date + code);
      var s = { id: id, date: date, ignored: false, late: /!$/.test(code), override: false };
      if (code === "U") {
        var m = DATA.misc[miscIdx++ % DATA.misc.length];
        s.kind = "U";
        s.title = m.s;
        s.agent = m.agent;
        s.projectId = null;
        s.suggest = m.suggest ? projByKey[m.suggest].id : null;
        s.source = m.source || null;
        s.item = { s: m.s, q: m.s + ".", p: "Answered with a short explanation and examples.", d: [] };
        s.dir = "~";
        s.branch = "";
      } else {
        var key = code[0];
        var num = parseInt(code.slice(1), 10);
        var proj = projByKey[key];
        s.kind = key;
        s.item = DATA.items[key][num];
        s.title = s.item.s;
        s.agent = proj.agents[num % proj.agents.length];
        s.projectId = proj.id;
        s.suggest = null;
        s.dir = proj.dir;
        s.branch = key === "D" ? "main" : "feat/" + (s.item.branch || s.title).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 24);
      }
      var details = s.item.d.length;
      s.minutes = 18 + (h % 47);
      s.turns = Math.max(details + 2, 3 + (h % 4));
      s.k = 1 + ((h >>> 5) % (s.turns - details));
      s.start = cursor;
      s.end = cursor + s.minutes;
      cursor = s.end + 12 + ((h >>> 9) % 55);
      slices.push(s);
    });
  });
  function sliceById(id) { return slices[id - 1] || null; }
  var datesWithSessions = Object.keys(DATA.schedule).sort();

  function turnsOf(s) {
    var out = [];
    var fill = DATA.filler[s.kind] || DATA.filler.U;
    var proj = projById(s.projectId) || (s.kind !== "U" ? projByKey[s.kind] : null);
    for (var i = 1; i <= s.turns; i++) {
      var t = { idx: i, time: hhmm(s.start + Math.round(((i - 1) * s.minutes) / s.turns)) };
      if (i === s.k) {
        t.q = s.item.q;
        t.a = s.item.p + ".";
        t.tools = proj ? ["Read " + proj.files[hash(s.title) % proj.files.length], "Edit " + proj.files[(hash(s.title) + 1) % proj.files.length] + " +" + (8 + (hash(s.title) % 60)) + " −" + (hash(s.title) % 14)] : [];
      } else if (i > s.k && i <= s.k + s.item.d.length) {
        t.q = FOLLOW_UPS[(i - s.k - 1) % FOLLOW_UPS.length];
        t.a = s.item.d[i - s.k - 1] + ".";
        t.tools = proj ? ["Edit " + proj.files[(hash(s.title) + i) % proj.files.length] + " +" + (3 + (i * 7) % 30)] : [];
      } else {
        var f = fill[(hash(s.title) + i) % fill.length];
        t.q = f.q; t.a = f.a; t.tools = f.t;
      }
      out.push(t);
    }
    return out;
  }

  /* ----------------------------------------------------------- periods */

  function notIgnored(list) { return list.filter(function (s) { return !s.ignored; }); }
  function slicesOn(date) { return slices.filter(function (s) { return s.date === date; }); }
  function weekDates(wk) {
    var monday = weekMondays[wk];
    var out = [];
    for (var i = 0; i < 7; i++) out.push(addDays(monday, i));
    return out;
  }
  function monthWeeks(m) {
    return Object.keys(weekMondays).filter(function (wk) { return weekMonth(weekMondays[wk]) === m; }).sort();
  }

  // Weeks from the first scheduled day up to the current week, plus synthetic history.
  var weekMondays = {};
  (function () {
    var d = mondayOf("2026-08-31");
    while (d <= TODAY) { weekMondays[weekKey(d)] = d; d = addDays(d, 7); }
  })();

  // Synthetic sessions before the detailed schedule starts, each in a project and at about the
  // pace of the detailed weeks, so the month list, the monthly reports' stats lines and the
  // project counts all agree.
  var FIRST_DETAILED = datesWithSessions[0];
  var HISTORY_START = mondayOf("2026-08-31");
  var HISTORY_MIX = ["B", "B", "B", "D", "D", "C", "C", "F", "F"]; // Billing is the main project.
  function syntheticDay(date) {
    if (date < HISTORY_START || date >= FIRST_DETAILED) return null;
    var wd = weekday(date);
    var h = hash("syn" + date);
    if ((wd === 0 || wd === 6) && h % 3 !== 0) return null;
    var out = [];
    for (var i = 0, n = 1 + (h % 4); i < n; i++) {
      var hi = hash("syn" + date + "#" + i);
      out.push({ date: date, minutes: 20 + (hi % 60), projectId: projByKey[HISTORY_MIX[hi % HISTORY_MIX.length]].id, ignored: false });
    }
    return out;
  }
  function syntheticSlices(dates) {
    return [].concat.apply([], dates.map(function (d) { return syntheticDay(d) || []; }));
  }

  function dayOverview(date) {
    if (date < FIRST_DETAILED) {
      var syn = syntheticDay(date);
      return syn ? { date: date, slices: syn.length, minutes: syn.reduce(function (a, s) { return a + s.minutes; }, 0), unassigned: 0, status: "final" } : null;
    }
    var list = slicesOn(date);
    if (!list.length) return null;
    var active = notIgnored(list);
    var rep = reports.daily[date];
    return {
      date: date,
      slices: active.length,
      minutes: active.reduce(function (a, s) { return a + s.minutes; }, 0),
      unassigned: active.filter(function (s) { return s.projectId === null; }).length,
      status: rep ? rep.status : null,
    };
  }
  function weekOverview(wk) {
    if (!weekMondays[wk]) return null; // A future week.
    var days = weekDates(wk).map(dayOverview).filter(Boolean);
    if (!days.length) return null;
    var rep = reports.weekly[wk];
    return {
      week: wk,
      monday: weekMondays[wk],
      days: days.length,
      slices: days.reduce(function (a, d) { return a + d.slices; }, 0),
      minutes: days.reduce(function (a, d) { return a + d.minutes; }, 0),
      unassigned: days.reduce(function (a, d) { return a + d.unassigned; }, 0),
      status: rep ? rep.status : null,
    };
  }
  function monthOverview(m) {
    var weeks = monthWeeks(m).map(weekOverview).filter(Boolean);
    if (!weeks.length) return null;
    var rep = reports.monthly[m];
    return {
      month: m,
      weeks: weeks,
      activeWeeks: weeks.length,
      reportedWeeks: weeks.filter(function (w) { return w.status; }).length,
      minutes: weeks.reduce(function (a, w) { return a + w.minutes; }, 0),
      unassigned: weeks.reduce(function (a, w) { return a + w.unassigned; }, 0),
      status: rep ? rep.status : null,
    };
  }

  /* ----------------------------------------------------------- reports */

  // Slices that go into a report: assigned to a project and not ignored.
  function reportable(list) {
    return list.filter(function (s) { return !s.ignored && s.projectId !== null; });
  }
  function byProject(list) {
    var groups = {};
    reportable(list).forEach(function (s) { (groups[s.projectId] = groups[s.projectId] || []).push(s); });
    return projects.filter(function (p) { return groups[p.id]; }).map(function (p) { return { project: p, slices: groups[p.id] }; });
  }
  function manifestOf(list) {
    var m = {};
    byProject(list).forEach(function (g) { m[g.project.name] = g.slices.map(function (s) { return s.id; }); });
    return m;
  }
  function itemLines(s) {
    var lines = ["- " + s.item.p + " [#S" + s.id + ".T" + s.k + "]"];
    s.item.d.forEach(function (d, j) { lines.push("  - " + d + " [#S" + s.id + ".T" + (s.k + 1 + j) + "]"); });
    return lines;
  }
  function dailyTitle(date) { return date + " · " + WD_LONG[weekday(date)]; }
  function weekTitle(wk) {
    var mon = weekMondays[wk];
    return wk + " · " + mon.slice(5) + " ~ " + addDays(mon, 6).slice(5);
  }
  // A month's weeks are the ISO weeks whose Thursday falls in it, including future ones.
  function monthTitle(m) {
    var weeks = [];
    for (var d = mondayOf(m + "-01"); addDays(d, 3).slice(0, 7) <= m; d = addDays(d, 7)) {
      if (addDays(d, 3).slice(0, 7) === m) weeks.push(weekKey(d));
    }
    return m + " · W" + weeks[0].slice(6) + " ~ W" + weeks[weeks.length - 1].slice(6);
  }
  function statsLine(list) {
    var days = {};
    list.forEach(function (s) { days[s.date] = true; });
    var mins = list.reduce(function (a, s) { return a + s.minutes; }, 0);
    return statsText(Object.keys(days).length, (mins / 60).toFixed(1), list.length);
  }
  function statsText(days, hours, sessions) {
    if (ZH) return "> 活跃 " + days + " 天 · 约 " + hours + " 小时 · " + sessions + " 个会话";
    return "> Active " + days + (days === 1 ? " day" : " days") + " · ~" + hours + " h · " + sessions + (sessions === 1 ? " session" : " sessions");
  }

  function monthDates(m) { return [].concat.apply([], monthWeeks(m).map(weekDates)); }
  function periodSlices(kind, key) {
    if (kind === "daily") return slicesOn(key);
    var dates = kind === "weekly" ? weekDates(key) : monthDates(key);
    return slices.filter(function (s) { return dates.indexOf(s.date) >= 0; });
  }

  function buildSections(kind, key, list) {
    if (kind === "monthly") {
      var hand = DATA.monthly[key] || {};
      var all = reportable(list).concat(syntheticSlices(monthDates(key)));
      return projects.filter(function (p) { return hand[p.key]; }).map(function (p) {
        var mine = all.filter(function (s) { return s.projectId === p.id; });
        return { name: p.name, lines: (mine.length ? [statsLine(mine)] : []).concat(hand[p.key].map(function (t) { return "- " + t; })) };
      });
    }
    return byProject(list).map(function (g) {
      var lines = kind === "weekly" ? [statsLine(g.slices)] : [];
      g.slices.forEach(function (s) { lines = lines.concat(itemLines(s)); });
      return { name: g.project.name, lines: lines };
    });
  }
  function titleOf(kind, key) {
    return kind === "daily" ? dailyTitle(key) : kind === "weekly" ? weekTitle(key) : monthTitle(key);
  }
  function serialize(title, sections) {
    var out = ["# " + title, ""];
    sections.forEach(function (sec) { out.push("## " + sec.name); out = out.concat(sec.lines); out.push(""); });
    return out.join("\n").trim() + "\n";
  }
  function splitMarkdown(md) {
    var title = "";
    var sections = [];
    md.split("\n").forEach(function (line) {
      if (/^# /.test(line) && !title && !sections.length) { title = line.slice(2).trim(); return; }
      if (/^## /.test(line)) { sections.push({ name: line.slice(3).trim(), lines: [] }); return; }
      if (sections.length && line.trim()) sections[sections.length - 1].lines.push(line);
    });
    return { title: title, sections: sections };
  }

  function reportManifest(kind, key, list) {
    if (kind !== "monthly") return manifestOf(list);
    var manifest = {};
    monthWeeks(key).forEach(function (week) {
      var rep = reports.weekly[week];
      if (!rep) return;
      splitMarkdown(rep.md).sections.forEach(function (sec) {
        var lines = sec.lines.filter(function (line) { return line.charAt(0) !== ">"; });
        if (lines.length) (manifest[sec.name] = manifest[sec.name] || []).push(week + "\n" + lines.join("\n"));
      });
    });
    return manifest;
  }

  var reports = { daily: {}, weekly: {}, monthly: {} };
  function makeReport(kind, key, opts) {
    opts = opts || {};
    var list = periodSlices(kind, key);
    var used = opts.excludeLate ? list.filter(function (s) { return !s.late; }) : list;
    return {
      md: serialize(titleOf(kind, key), buildSections(kind, key, used)),
      v: 1,
      status: opts.status || "final",
      manifest: reportManifest(kind, key, used),
    };
  }
  (function seedReports() {
    var rs = DATA.reportState;
    datesWithSessions.forEach(function (d) {
      var st = d in rs.daily ? rs.daily[d] : "final";
      if (st) reports.daily[d] = makeReport("daily", d, { status: st, excludeLate: true });
    });
    Object.keys(weekMondays).forEach(function (wk) {
      if (weekMondays[wk] < mondayOf(FIRST_DETAILED)) { reports.weekly[wk] = { md: "", v: 1, status: "final", manifest: null, synthetic: true }; return; }
      var st = wk in rs.weekly ? rs.weekly[wk] : "final";
      if (st) reports.weekly[wk] = makeReport("weekly", wk, { status: st, excludeLate: true });
    });
    Object.keys(DATA.monthly).forEach(function (m) {
      var st = m in rs.monthly ? rs.monthly[m] : "final";
      if (st) reports.monthly[m] = makeReport("monthly", m, { status: st });
    });
    // Two versions already exist for a couple of days, as after a prompt tweak.
    ["2026-11-12", "2026-11-16"].forEach(function (d) { if (reports.daily[d]) reports.daily[d].v = 2; });
  })();

  function staleness(kind, key) {
    var rep = reports[kind][key];
    if (!rep || !rep.manifest) return null;
    var now = reportManifest(kind, key, periodSlices(kind, key));
    var names = {};
    Object.keys(now).concat(Object.keys(rep.manifest)).forEach(function (n) { names[n] = true; });
    var changed = [];
    var count = 0;
    Object.keys(names).forEach(function (n) {
      var a = now[n] || [];
      var b = rep.manifest[n] || [];
      var diff = a.filter(function (x) { return b.indexOf(x) < 0; }).length + b.filter(function (x) { return a.indexOf(x) < 0; }).length;
      if (diff) { changed.push(n); count += diff; }
    });
    if (!changed.length) return null;
    changed.sort(function (x, y) { return projOrder(x) - projOrder(y); });
    return { projects: changed, count: count };
  }
  function projOrder(name) {
    for (var i = 0; i < projects.length; i++) if (projects[i].name === name) return i;
    return 99;
  }

  /* ------------------------------------------------------------- state */

  var state = {
    mode: "daily",
    stats: false,
    date: "2026-11-18",
    week: CUR_WEEK,
    month: CUR_MONTH,
    projectId: projByKey.B.id,
    tab: { daily: "report", weekly: "report", monthly: "report", projects: "report" },
    editing: false,
    filter: "all",
    granularity: "daily",
    drawer: null,
    jobs: {},
    batches: { daily: null, weekly: null, monthly: null },
    expanded: {},
    foldedNotes: {},
    menu: null,
    source: "both",
    range: "30d",
    oldestFirst: false,
    manage: null,
    showAllSuggestions: false,
    toast: null,
    scanning: false,
    scanNote: null,
  };
  // Start with the first point of the first project open, so the outline's depth is visible.
  state.expanded["daily|2026-11-18|0|0"] = true;

  function periodKey(kind) { return kind === "daily" ? state.date : kind === "weekly" ? state.week : state.month; }

  /* ----------------------------------------------------------- render: bits */

  var ICON = 'width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"';

  // Toolbar icons, the same drawings as the desktop app (app/src/components/icons.tsx).
  function svg(body, width) {
    var w = width || 16;
    return '<svg width="' + w + '" height="16" viewBox="0 0 ' + w + ' 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + body + "</svg>";
  }
  var DOT = ' fill="currentColor" stroke="none"';
  var AGENT_G = '<rect x="2.5" y="5" width="11" height="8.5" rx="2.5"/><path d="M8 5V2.8"/><circle cx="8" cy="2.2" r="0.7"' + DOT + '/><circle cx="5.9" cy="9.2" r="0.95"' + DOT + '/><circle cx="10.1" cy="9.2" r="0.95"' + DOT + "/>";
  var HUMAN_G = '<circle cx="8" cy="5" r="2.7"/><path d="M2.8 14c0-2.9 2.3-5.1 5.2-5.1s5.2 2.2 5.2 5.1"/>';
  var CAL = '<rect x="2" y="3" width="12" height="11" rx="2"/><path d="M2 6.5h12M5.5 1.8v2.4M10.5 1.8v2.4"/>';
  var IC = {
    agent: svg('<g class="dm-g-agent">' + AGENT_G + "</g>"),
    human: svg('<g class="dm-g-human">' + HUMAN_G + "</g>"),
    both: svg('<g class="dm-g-agent" transform="translate(-1.5 0.5)">' + AGENT_G + '</g><g class="dm-g-human" transform="translate(12 0)">' + HUMAN_G + "</g>", 27),
    eye: svg('<path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8s-2.4 4.5-6.5 4.5S1.5 8 1.5 8z"/><circle cx="8" cy="8" r="1.9"/>'),
    pencil: svg('<path d="M10.8 2.8l2.4 2.4L6 12.4l-3.1.7.7-3.1z"/><path d="M9.4 4.2l2.4 2.4"/>'),
    more: svg('<circle cx="3.5" cy="8" r="1.1"' + DOT + '/><circle cx="8" cy="8" r="1.1"' + DOT + '/><circle cx="12.5" cy="8" r="1.1"' + DOT + "/>"),
    newest: svg('<path d="M5 3v10M2.5 10.5L5 13l2.5-2.5M9 5h3M9 8h4M9 11h5"/>'),
    oldest: svg('<path d="M5 13V3M2.5 5.5L5 3l2.5 2.5M9 5h5M9 8h4M9 11h3"/>'),
    day: svg(CAL + '<rect x="8.6" y="8.6" width="3" height="3" rx="0.7"' + DOT + "/>"),
    week: svg(CAL + '<rect x="4.2" y="8.9" width="7.6" height="2.4" rx="0.8"' + DOT + "/>"),
    month: svg(CAL + [5, 8, 11].map(function (x) { return [8.9, 11.6].map(function (y) { return '<circle cx="' + x + '" cy="' + y + '" r="0.85"' + DOT + "/>"; }).join(""); }).join("")),
    projects: svg('<path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.8l1.4 1.5h4.8A1.5 1.5 0 0 1 14 6v6a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12z"/>'),
    target: '<svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="1.6"' + DOT + "/></svg>",
  };
  /** A hover label (and accessible name) for an icon button. */
  function tip(label, align) {
    return ' data-tip="' + esc(label) + '" aria-label="' + esc(label) + '"' + (align ? ' data-tip-align="' + align + '"' : "");
  }
  /** Every label in one grid cell, so the button keeps the width of the longest. */
  function swap(labels, current) {
    return '<span class="dm-swap">' + labels.map(function (l) { return "<span" + (l === current ? "" : ' class="ghost" aria-hidden="true"') + ">" + esc(l) + "</span>"; }).join("") + "</span>";
  }
  var SOURCES = [["both", "Agent and you"], ["agent", "Agent only"], ["human", "Only what you wrote"]];
  function sourceFilter(disabled) {
    var html = '<div class="dm-pills dm-icon-pills dm-source" role="radiogroup" aria-label="Show">';
    SOURCES.forEach(function (f) {
      html += '<button type="button" role="radio" aria-checked="' + (state.source === f[0]) + '" class="dm-pill' + (state.source === f[0] ? " active" : "") + '" data-act="source" data-source="' + f[0] + '"' + tip(f[1]) + (disabled ? " disabled" : "") + ">" + IC[f[0]] + "</button>";
    });
    return html + "</div>";
  }
  /** The "⋯" menu at the right end of every toolbar; `items` are [label, act, extra attrs]. */
  function moreMenu(items) {
    var html = '<div class="dm-menu-wrap"><button type="button" class="dm-btn dm-btn-ghost dm-icon-btn" data-act="menu" aria-expanded="' + (state.menu === "options") + '"' + tip("Options", "end") + ">" + IC.more + "</button>";
    if (state.menu === "options") {
      html += '<div class="dm-menu">';
      items.forEach(function (it) {
        if (it === "-") html += '<div class="dm-menu-sep"></div>';
        else html += '<button type="button" data-act="' + it[1] + '"' + (it[2] || "") + ">" + esc(it[0]) + "</button>";
      });
      html += "</div>";
    }
    return html + "</div>";
  }
  var RANGES = [["7d", "Last 7 days"], ["14d", "Last 14 days"], ["30d", "Last 30 days"], ["all", "All time"]];
  function rangeSelect() {
    var html = '<select class="dm-select" data-act="range" aria-label="Time range">';
    RANGES.forEach(function (r) { html += '<option value="' + r[0] + '"' + (state.range === r[0] ? " selected" : "") + ">" + r[1] + "</option>"; });
    return html + "</select>";
  }
  /** First date inside the selected project range, or null for all time. */
  function rangeStart() {
    return state.range === "all" ? null : addDays(TODAY, -(parseInt(state.range, 10) - 1));
  }

  function statusDot(status, job) {
    if (job) return '<span class="dm-dot generating ' + job + '" title="Generating"></span>';
    return '<span class="dm-dot ' + (status || "none") + '" title="' + (status ? "Report: " + status : "No report yet") + '"></span>';
  }
  // Both labels share one grid cell, so the badge keeps the width of the longer one when toggled.
  function statusLabel(status) {
    var cell = function (label, on) { return "<span" + (on ? "" : ' class="ghost" aria-hidden="true"') + ">" + label + "</span>"; };
    return '<span class="dm-swap">' + cell("Draft", status === "draft") + cell("Final", status === "final") + "</span>";
  }
  function level(mins, max) { return mins ? Math.min(4, Math.ceil((mins / max) * 4)) : 0; }
  function agentBadge(agent) { return '<span class="dm-agent">' + esc(DATA.agents[agent]) + "</span>"; }

  function renderInline(text) {
    return esc(text).replace(/\[#S(\d+)\.T(\d+)\]/g, function (_, s, t) {
      return '<button type="button" class="dm-anchor" data-act="anchor" data-slice="' + s + '" data-turn="' + t + '" title="Open slice S' + s + " at turn T" + t + '">[#S' + s + ".T" + t + "]</button>";
    });
  }

  // Parse one section's lines into a quote and an outline of top-level points with details.
  function outlineOf(lines) {
    var quote = null;
    var nodes = [];
    lines.forEach(function (line) {
      if (/^> /.test(line)) { quote = line.slice(2); return; }
      var m = /^(\s*)[-*] (.*)$/.exec(line);
      if (!m) return;
      if (m[1].length >= 2 && nodes.length) nodes[nodes.length - 1].children.push(m[2]);
      else nodes.push({ text: m[2], children: [] });
    });
    return { quote: quote, nodes: nodes };
  }

  // `ann` maps a point's index to the annotations hanging on it (see notes below).
  function renderOutline(lines, idPrefix, expandAll, ann, noteId) {
    var o = outlineOf(lines);
    var html = o.quote ? '<p class="dm-quote">' + esc(o.quote) + "</p>" : "";
    html += '<ul class="dm-outline">';
    o.nodes.forEach(function (n, i) {
      var id = idPrefix + "|" + i;
      var has = n.children.length > 0;
      var open = has && (expandAll || state.expanded[id]);
      html += '<li><div class="dm-orow">';
      html += has
        ? '<button type="button" class="dm-otoggle' + (open ? " open" : "") + '" data-act="toggle" data-id="' + esc(id) + '" aria-expanded="' + open + '" aria-label="' + (open ? "Collapse" : "Expand") + ' details"><svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.25 3.25L11.25 8L5.25 12.75V3.25Z" fill="currentColor"/></svg></button>'
        : '<span class="dm-oleaf"><i></i></span>';
      html += '<div class="dm-otext' + (has ? " clickable" : "") + '"' + (has ? ' data-act="toggle" data-id="' + esc(id) + '"' : "") + ">" + renderInline(n.text) + "</div></div>";
      if (open) {
        html += '<ul class="dm-sublist">';
        n.children.forEach(function (c) { html += '<li><div class="dm-orow"><span class="dm-oleaf sub"><i></i></span><div class="dm-otext">' + renderInline(c) + "</div></div></li>"; });
        html += "</ul>";
      }
      if (ann && ann[i]) html += noteBox(ann[i], noteId + ".a" + i, "dm-hann", true);
      html += "</li>";
    });
    html += "</ul>";
    // Annotations whose point is gone (a session moved away) stay, at the end of the section.
    if (ann) Object.keys(ann).forEach(function (k) { if (+k >= o.nodes.length) html += noteBox(ann[k], noteId + ".a" + k, "dm-hann", true); });
    return html;
  }

  /* -------------------------------------------------- render: human notes */

  // What the person wrote, read-only in the demo. Drawn like the app's notes: the report's
  // outline style on a light tint of the human color. Agent only hides them; Only what you
  // wrote shows them alone, with each annotated point as a faint line of context.
  var NOTES = DATA.notes || {};
  function periodNotes(kind, key) { return (NOTES[kind] && NOTES[kind][key]) || {}; }
  function projectNotes(kind, key, pkey) { var n = periodNotes(kind, key).projects; return (n && n[pkey]) || {}; }
  function hasNotes(pn) { return !!((pn.top && pn.top.length) || (pn.bottom && pn.bottom.length) || (pn.ann && Object.keys(pn.ann).length)); }
  // A note with children shows a triangle instead of its dot and folds like a report point; as
  // in the app, notes start unfolded. `id` names the list so each note's fold state is kept.
  function noteItems(list, sub, id) {
    var html = '<ul class="dm-hlist">';
    list.forEach(function (it, i) {
      var text = typeof it === "string" ? it : it[0];
      var kids = typeof it === "string" ? [] : it[1];
      var nid = id + "." + i;
      var open = kids.length > 0 && !state.foldedNotes[nid];
      html += '<li><div class="dm-hrow">';
      html += kids.length
        ? '<button type="button" class="dm-otoggle dm-htoggle' + (open ? " open" : "") + '" data-act="note-fold" data-id="' + esc(nid) + '" aria-expanded="' + open + '" aria-label="' + (open ? "Collapse" : "Expand") + '"><svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.25 3.25L11.25 8L5.25 12.75V3.25Z" fill="currentColor"/></svg></button>'
        : '<span class="dm-hbullet' + (sub ? " sub" : "") + '"><i></i></span>';
      html += '<div class="dm-htext">' + renderInline(text) + "</div></div>";
      if (open) html += '<div class="dm-hkids">' + noteItems(kids, true, nid) + "</div>";
      html += "</li>";
    });
    return html + "</ul>";
  }
  function noteBox(list, id, cls, sub) {
    return list && list.length ? '<div class="dm-hbox' + (cls ? " " + cls : "") + '">' + noteItems(list, sub, id) + "</div>" : "";
  }
  function annotatedContext(lines, ann, noteId) {
    var nodes = lines ? outlineOf(lines).nodes : [];
    return Object.keys(ann).map(function (k) {
      var n = nodes[+k];
      return (n ? '<div class="dm-hctx">' + renderInline(n.text) + "</div>" : "") + noteBox(ann[k], noteId + ".a" + k, "dm-hann", true);
    }).join("");
  }
  // One project section: notes at the top, the generated outline with its annotations, notes
  // at the bottom. `lines` is null for a project the report has no section for.
  // `noteId` identifies the period and project, so a note folds the same in its report and on
  // the project page.
  function sectionBody(lines, idPrefix, expandAll, pn, source, noteId) {
    if (source === "human") return noteBox(pn.top, noteId + ".t", "dm-htop") + (pn.ann ? annotatedContext(lines, pn.ann, noteId) : "") + noteBox(pn.bottom, noteId + ".b", "dm-hbottom");
    return noteBox(pn.top, noteId + ".t", "dm-htop") + (lines ? renderOutline(lines, idPrefix, expandAll, pn.ann, noteId) : detachedNotes(pn.ann, noteId)) + noteBox(pn.bottom, noteId + ".b", "dm-hbottom");
  }
  // Annotations whose report section is missing: shown on their own rather than lost.
  function detachedNotes(ann, noteId) {
    return ann ? Object.keys(ann).map(function (k) { return noteBox(ann[k], noteId + ".a" + k, "dm-hann", true); }).join("") : "";
  }
  function projectCard(name, p, inner) {
    var html = '<section class="dm-card"><div class="dm-card-head"><span class="dm-bar"></span><h4>' + esc(name) + "</h4>";
    if (p) html += '<button type="button" class="dm-btn dm-btn-ghost dm-btn-xs dm-card-act" data-act="open-project" data-project="' + p.id + '">Open Project Report -&gt;</button>';
    return html + "</div>" + inner + "</section>";
  }
  // Day notes, then a card per project: the report's sections (if any) merged with the notes,
  // then cards for projects that have notes but no section in this report.
  function periodBody(kind, key, sections, expandAll, source) {
    var notes = source === "agent" ? {} : periodNotes(kind, key);
    var html = notes.day && notes.day.length ? '<div class="dm-hday"><div class="dm-hhead">Notes</div>' + noteBox(notes.day, kind + "|" + key + ".day") + "</div>" : "";
    var seen = {};
    sections.forEach(function (sec, si) {
      var p = projects.filter(function (x) { return x.name === sec.name; })[0];
      var pn = p && source !== "agent" ? projectNotes(kind, key, p.key) : {};
      if (p) seen[p.key] = true;
      if (source === "human" && !hasNotes(pn)) return;
      html += projectCard(sec.name, p, sectionBody(sec.lines, kind + "|" + key + "|" + si, expandAll, pn, source, kind + "|" + key + "|" + (p ? p.key : si)));
    });
    if (source !== "agent") projects.forEach(function (p) {
      var pn = projectNotes(kind, key, p.key);
      if (!seen[p.key] && hasNotes(pn)) html += projectCard(p.name, p, sectionBody(null, "", false, pn, source, kind + "|" + key + "|" + p.key));
    });
    return html;
  }

  function hasNested(md) { return /\n\s{2,}[-*] /.test(md); }
  function allExpanded(kind, key) { return !!state.expanded["all|" + kind + "|" + key]; }

  /* -------------------------------------------------------- render: sidebar */

  function renderSidebar() {
    var mode = state.mode;
    var html = '<aside class="dm-side">';
    var viewTab = function (t, align) {
      var on = mode === t[0] && !state.stats;
      return '<button type="button" role="tab" aria-selected="' + on + '" class="dm-pill' + (on ? " active" : "") + '" data-act="mode" data-mode="' + t[0] + '"' + tip(t[1], align) + ">" + IC[t[2]] + "</button>";
    };
    html += '<div class="dm-view-switch" role="tablist" aria-label="View"><div class="dm-pills">';
    html += viewTab(["daily", "Daily", "day"], "start") + viewTab(["weekly", "Weekly", "week"]) + viewTab(["monthly", "Monthly", "month"]);
    html += '</div><span class="dm-sep"></span><div class="dm-pills dm-view-projects">' + viewTab(["projects", "Projects", "projects"], "end") + "</div></div>";
    if (mode !== "projects") html += renderCalendar();
    if (mode === "daily") html += renderDayList();
    else if (mode === "weekly") html += renderWeekList();
    else if (mode === "monthly") html += renderMonthList();
    else html += renderProjectList();
    if (mode !== "projects" && state.batches[mode]) html += renderBatchBar(mode);
    html += '<div class="dm-side-foot">';
    if (state.scanNote) html += '<div class="dm-note">' + esc(state.scanNote) + "</div>";
    html += '<div class="dm-foot-actions">';
    html += '<button type="button" class="dm-btn dm-btn-secondary dm-btn-sm grow" data-act="scan"' + (state.scanning ? " disabled" : "") + ">" + (state.scanning ? "Scanning..." : "Scan") + "</button>";
    html += '<button type="button" class="dm-btn dm-btn-secondary dm-btn-sm dm-sq' + (state.stats ? " active" : "") + '" data-act="stats" title="Stats" aria-label="Stats" aria-pressed="' + state.stats + '"><svg ' + ICON + '><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></svg></button>';
    html += '<button type="button" class="dm-btn dm-btn-secondary dm-btn-sm dm-sq" data-act="unavailable" title="Settings" aria-label="Settings"><svg ' + ICON + '><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1"/></svg></button>';
    html += "</div></div></aside>";
    return html;
  }

  function renderCalendar() {
    var mode = state.mode;
    var html = '<div class="dm-section dm-cal">';
    var label = mode === "monthly" ? CUR_MONTH.slice(0, 4) : SHOWN_MONTH;
    var jump = mode === "monthly" ? "Jump to this month" : mode === "weekly" ? "Jump to this week" : "Jump to today";
    var atCurrent = mode === "monthly" ? state.month === CUR_MONTH : mode === "weekly" ? state.week === CUR_WEEK : state.date === TODAY;
    html += '<div class="dm-cal-head"><span class="dm-cal-title">' + label + '</span><div class="dm-cal-nav">';
    html += '<button type="button" class="dm-iconbtn" disabled title="The demo stays on this ' + (mode === "monthly" ? "year" : "month") + '">&lt;</button>';
    html += '<button type="button" class="dm-iconbtn dm-today" data-act="jump"' + tip(jump, "end") + (atCurrent ? " disabled" : "") + ">" + IC.target + "</button>";
    html += '<button type="button" class="dm-iconbtn" disabled title="The demo stays on this ' + (mode === "monthly" ? "year" : "month") + '">&gt;</button>';
    html += "</div></div>";

    var cells = [];
    for (var i = 0; i < 42; i++) cells.push(addDays(GRID_START, i));
    var dayMax = 60;
    cells.forEach(function (d) { var o = d <= TODAY && dayOverview(d); if (o) dayMax = Math.max(dayMax, o.minutes); });
    function dayLevel(d) { var o = d <= TODAY && dayOverview(d); return o ? level(o.minutes, dayMax) : 0; }

    if (mode === "daily") {
      html += '<div class="dm-cal-grid">';
      WD_GRID.forEach(function (w) { html += '<div class="dm-wd">' + w + "</div>"; });
      cells.forEach(function (d) {
        var o = d <= TODAY && dayOverview(d);
        var cls = "dm-day lv-" + dayLevel(d) + (d.slice(0, 7) !== SHOWN_MONTH ? " outside" : "") + (d === state.date ? " selected" : "") + (d === TODAY ? " today" : "");
        var title = o ? d + ": " + o.slices + " slices, " + o.minutes + " min" + (o.unassigned ? ", " + o.unassigned + " unassigned" : "") : d;
        html += '<button type="button" class="' + cls + '" data-act="day" data-date="' + d + '" title="' + title + '"' + (d > TODAY ? " disabled" : "") + "><span>" + parseInt(d.slice(8), 10) + "</span></button>";
      });
      html += "</div>";
    } else if (mode === "weekly") {
      var wkMax = 60;
      Object.keys(weekMondays).forEach(function (wk) { var o = weekOverview(wk); if (o) wkMax = Math.max(wkMax, o.minutes); });
      html += '<div class="dm-week-grid"><div class="dm-week-row header"><span class="dm-wd">W</span>';
      WD_GRID.forEach(function (w) { html += '<span class="dm-wd">' + w + "</span>"; });
      html += "</div>";
      for (var r = 0; r < 6; r++) {
        var mon = cells[r * 7];
        var wk = weekKey(mon);
        var wo = weekOverview(wk);
        var future = mon > TODAY;
        html += '<button type="button" class="dm-week-row' + (wk === state.week ? " selected" : "") + (wk === CUR_WEEK ? " current" : "") + '" data-act="week" data-week="' + wk + '"' + (future || !wo ? " disabled" : "") + ' title="' + wk + (wo ? ": " + wo.days + " active days, " + wo.minutes + " min" : "") + '">';
        html += '<span class="dm-wnum lv-' + (wo ? level(wo.minutes, wkMax) : 0) + '">W' + wk.slice(6) + "</span>";
        for (var c = 0; c < 7; c++) {
          var d2 = cells[r * 7 + c];
          html += '<span class="dm-day lv-' + dayLevel(d2) + (d2.slice(0, 7) !== SHOWN_MONTH ? " outside" : "") + '">' + parseInt(d2.slice(8), 10) + "</span>";
        }
        html += "</button>";
      }
      html += "</div>";
    } else {
      var year = CUR_MONTH.slice(0, 4);
      var moMax = 60;
      MONTHS.forEach(function (_, i) { var o = monthOverview(year + "-" + pad(i + 1)); if (o) moMax = Math.max(moMax, o.minutes); });
      html += '<div class="dm-month-grid">';
      MONTHS.forEach(function (label2, i) {
        var key = year + "-" + pad(i + 1);
        var o = monthOverview(key);
        html += '<button type="button" class="dm-day dm-month-cell lv-' + (o ? level(o.minutes, moMax) : 0) + (key === state.month ? " selected" : "") + (key === CUR_MONTH ? " today" : "") + '" data-act="month" data-month="' + key + '"' + (o ? "" : " disabled") + ' title="' + key + (o ? ": " + o.reportedWeeks + "/" + o.activeWeeks + " weekly reports, " + o.minutes + " min" : "") + '"><span>' + label2 + "</span></button>";
      });
      html += "</div>";
    }
    return html + "</div>";
  }

  function rowShell(active, dateHtml, metaHtml, act, attrs) {
    return '<button type="button" class="dm-row' + (active ? " active" : "") + '" data-act="' + act + '" ' + attrs + '><span class="dm-row-date">' + dateHtml + '</span><span class="dm-row-meta">' + metaHtml + "</span></button>";
  }

  // Top-level notes on a day: day notes, project notes and annotations; children don't count.
  function noteCount(date) {
    var n = periodNotes("daily", date), c = (n.day || []).length;
    Object.keys(n.projects || {}).forEach(function (k) {
      var pn = n.projects[k];
      c += (pn.top || []).length + (pn.bottom || []).length;
      Object.keys(pn.ann || {}).forEach(function (i) { c += pn.ann[i].length; });
    });
    return c;
  }

  function batchListHeader(kind, label) {
    var open = state.menu === "batch-list";
    var html = '<div class="dm-label-row"><span class="dm-label">' + label + '</span><div class="dm-menu-wrap">';
    html += '<button type="button" class="dm-btn dm-btn-ghost dm-btn-xs" data-act="menu" data-menu="batch-list" aria-label="' + label + ' list options" aria-expanded="' + open + '">⋯</button>';
    if (open) {
      var options = [
        [state.batches[kind] ? "Exit multi-select" : "Select " + label.toLowerCase() + "…", "select"],
        ["All drafts", "drafts"], ["All out of date", "stale"], ["All without a report", "missing"],
      ];
      if (kind === "daily") options.splice(1, 0, ["This week", "week"], ["This month", "month"]);
      html += '<div class="dm-menu">';
      options.forEach(function (opt) {
        html += '<button type="button" data-act="batch-select" data-pick="' + opt[1] + '">' + opt[0] + '</button>';
      });
      html += '</div>';
    }
    return html + '</div></div>';
  }

  function batchRow(kind, key, active, dateHtml, metaHtml, act, attrs) {
    var batch = state.batches[kind];
    var row = rowShell(active, dateHtml, metaHtml, act, attrs + ' data-list-key="' + key + '"');
    if (!batch || batch.running) return row;
    var checked = batch.keys.indexOf(key) >= 0;
    return '<div class="dm-batch-row' + (checked ? ' selected' : '') + '"><input type="checkbox" aria-label="Select ' + key +
      '" data-act="batch-toggle" data-key="' + key + '"' + (checked ? ' checked' : '') + '>' + row + '</div>';
  }

  function batchItem(kind, key, includeFinal) {
    var rep = reports[kind][key];
    var month = kind === "monthly" && monthOverview(key);
    var available = month ? month.reportedWeeks > 0 : reportable(periodSlices(kind, key)).length > 0;
    var eligible = available && (!rep || rep.status !== "final" || includeFinal);
    return {
      key: key, eligible: eligible, missing: eligible && !rep, stale: eligible && !!staleness(kind, key),
      missingWeeks: month ? month.weeks.filter(function (w) { return !w.status; }).map(function (w) { return w.week; }) : [],
      available: available,
    };
  }

  function renderBatchBar(kind) {
    var batch = state.batches[kind];
    var plan = batch.keys.map(function (key) { return batchItem(kind, key, batch.includeFinal); });
    function button(label, action, value, disabled, style) {
      return '<button type="button" class="dm-btn dm-btn-xs dm-btn-' + (style || "secondary") + '" data-act="' +
        action + '" data-value="' + value + '"' + (disabled ? ' disabled' : '') + '>' + label + '</button>';
    }
    var html = '<div class="dm-batch-bar">';
    if (batch.running) return html + '<div>Running ' + batch.running.done + '/' + batch.running.total +
      button("Cancel all", "batch-cancel", "", false, "danger") + '</div></div>';
    var missing = plan.filter(function (p) { return p.missing; }).length;
    var stale = plan.filter(function (p) { return p.stale; }).length;
    var eligible = plan.filter(function (p) { return p.eligible; }).length;
    html += '<div>' + batch.keys.length + ' selected' + button("×", "batch-close", "", false, "ghost") + '</div>';
    html += '<div>' + button("Generate / Update", "batch-run", "smart", !missing && !stale, "primary") +
      button("▾", "menu", "batch-actions", !plan.length) + button("Final", "batch-status", "final", !plan.length) +
      button("Draft", "batch-status", "draft", !plan.length) + '</div>';
    if (state.menu === "batch-actions") {
      html += '<div class="dm-batch-options">' + button("Update out of date (" + stale + ")", "batch-run", "stale", !stale) +
        button("Generate missing (" + missing + ")", "batch-run", "missing", !missing) +
        button("Regenerate all (" + eligible + ")", "batch-run", "regenerate", !eligible) +
        button((batch.includeFinal ? "✓ " : "") + "Include final", "batch-final", "", false, "ghost") + '</div>';
    }
    if (batch.confirm) html += '<div>Regenerate ' + eligible + ' reports?' +
      button("Confirm", "batch-run", "confirmed", !eligible) + button("Cancel", "batch-dismiss", "", false) + '</div>';
    if (kind === "monthly") plan.forEach(function (p) {
      if (p.missingWeeks.length) html += '<p>' + p.key + (p.available ? ' is missing ' : ' will be skipped. Missing ') +
        p.missingWeeks.join(", ") + '</p>';
    });
    if (batch.summary) html += '<p role="status">' + esc(batch.summary) + '</p>';
    return html + '</div>';
  }

  function renderDayList() {
    var days = [];
    for (var d = TODAY; d >= GRID_START; d = addDays(d, -1)) { var o = dayOverview(d); if (o) days.push(o); }
    var html = '<div class="dm-section dm-list-wrap">' + batchListHeader("daily", "Days") + '<div class="dm-list" data-scroll="days">';
    days.forEach(function (o) {
      // As in the app: how many notes the person wrote, then time. (The narrow demo sidebar
      // leaves unassigned sessions to the Sessions tab.)
      var nn = noteCount(o.date);
      var meta = (nn ? '<span class="dm-row-notes" title="' + nn + (nn === 1 ? " note" : " notes") + '">' + nn + "</span>" : "") + '<span class="dm-stats" title="' + o.slices + (o.slices === 1 ? " session" : " sessions") + '">' + fmtMinutes(o.minutes) + "</span>" + statusDot(o.status, state.jobs["daily|" + o.date]);
      html += batchRow("daily", o.date, o.date === state.date, o.date.slice(5) + '<span class="dm-row-wd">' + (o.date === TODAY ? "Today" : WD_SHORT[weekday(o.date)]) + "</span>", meta, "day", 'data-date="' + o.date + '"');
    });
    return html + "</div></div>";
  }
  function renderWeekList() {
    var html = '<div class="dm-section dm-list-wrap">' + batchListHeader("weekly", "Weeks") + '<div class="dm-list" data-scroll="weeks">';
    Object.keys(weekMondays).sort().reverse().forEach(function (wk) {
      if (weekMondays[wk] < GRID_START) return;
      var o = weekOverview(wk);
      if (!o) return;
      var sub = wk === CUR_WEEK ? "This week" : o.monday.slice(5) + " ~ " + addDays(o.monday, 6).slice(5);
      var meta = (o.unassigned ? '<span class="dm-badge warn">' + o.unassigned + "</span>" : "") + '<span class="dm-stats">' + o.days + "d · " + fmtMinutes(o.minutes) + "</span>" + statusDot(o.status, state.jobs["weekly|" + wk]);
      html += batchRow("weekly", wk, wk === state.week, wk.slice(5) + '<span class="dm-row-wd">' + sub + "</span>", meta, "week", 'data-week="' + wk + '"');
    });
    return html + "</div></div>";
  }
  function renderMonthList() {
    var html = '<div class="dm-section dm-list-wrap">' + batchListHeader("monthly", "Months") + '<div class="dm-list" data-scroll="months">';
    Object.keys(DATA.monthly).sort().reverse().forEach(function (m) {
      var o = monthOverview(m);
      if (!o) return;
      var sub = m === CUR_MONTH ? "This month" : monthTitle(m).split(" · ")[1];
      var meta = (o.unassigned ? '<span class="dm-badge warn">' + o.unassigned + "</span>" : "") + '<span class="dm-stats">' + o.reportedWeeks + "/" + o.activeWeeks + "w · " + fmtMinutes(o.minutes) + "</span>" + statusDot(o.status, state.jobs["monthly|" + m]);
      html += batchRow("monthly", m, m === state.month, m + '<span class="dm-row-wd">' + sub + "</span>", meta, "month", 'data-month="' + m + '"');
    });
    return html + "</div></div>";
  }
  // Synthetic sessions per project, counted once.
  var historyCounts = null;
  function historyCount(projectId) {
    if (!historyCounts) {
      historyCounts = {};
      for (var d = HISTORY_START; d < FIRST_DETAILED; d = addDays(d, 1)) {
        (syntheticDay(d) || []).forEach(function (s) { historyCounts[s.projectId] = (historyCounts[s.projectId] || 0) + 1; });
      }
    }
    return historyCounts[projectId] || 0;
  }
  function renderProjectList() {
    var html = '<div class="dm-section dm-list-wrap"><div class="dm-label-row"><span class="dm-label">Projects</span><button type="button" class="dm-btn dm-btn-ghost dm-btn-xs" data-act="unavailable">+ New</button></div><div class="dm-list" data-scroll="projects">';
    projects.forEach(function (p) {
      var n = slices.filter(function (s) { return s.projectId === p.id; }).length + historyCount(p.id);
      html += rowShell(p.id === state.projectId, esc(p.name), "<span>" + n + "</span>", "project", 'data-project="' + p.id + '"');
    });
    return html + "</div></div>";
  }

  /* ------------------------------------------------------- render: report */

  function tabs(kind, items) {
    var html = '<div class="dm-pills" role="tablist">';
    items.forEach(function (t) {
      html += '<button type="button" role="tab" aria-selected="' + (state.tab[kind] === t[0]) + '" class="dm-pill' + (state.tab[kind] === t[0] ? " active" : "") + '" data-act="tab" data-kind="' + kind + '" data-tab="' + t[0] + '">' + t[1] + "</button>";
    });
    return html + "</div>";
  }

  function renderPeriod(kind) {
    var key = periodKey(kind);
    var tabsHtml = tabs(kind, [["report", "Report"], ["sessions", "Sessions"]]);
    if (state.tab[kind] === "sessions") return renderSessions(kind, key, tabsHtml);
    return renderReport(kind, key, tabsHtml);
  }

  function renderReport(kind, key, tabsHtml) {
    var rep = reports[kind][key];
    var job = state.jobs[kind + "|" + key];
    var stale = !job && staleness(kind, key);
    var list = periodSlices(kind, key);
    var html = '<section class="dm-panel"><header class="dm-panel-head"><div class="dm-head-left">' + tabsHtml + '</div><div class="dm-head-right">';
    // The same controls in the same places whatever the state; unavailable ones are disabled.
    html += sourceFilter(state.editing);
    var noDoc = !rep || !!job;
    html += '<div class="dm-pills dm-icon-pills" role="radiogroup" aria-label="Reading or editing">';
    html += '<button type="button" role="radio" aria-checked="' + !state.editing + '" class="dm-pill' + (!state.editing ? " active" : "") + '" data-act="edit" data-on="0"' + tip("Preview") + (noDoc ? " disabled" : "") + ">" + IC.eye + "</button>";
    html += '<button type="button" role="radio" aria-checked="' + state.editing + '" class="dm-pill' + (state.editing ? " active" : "") + '" data-act="edit" data-on="1"' + tip("Edit") + (noDoc ? " disabled" : "") + ">" + IC.pencil + "</button></div>";
    var labels = ["Generate", "Regenerate", "Update", "Cancel"];
    if (job) html += '<button type="button" class="dm-btn dm-btn-danger dm-btn-sm" data-act="cancel">' + swap(labels, "Cancel") + "</button>";
    else if (stale) html += '<button type="button" class="dm-btn dm-btn-primary dm-btn-sm" data-act="generate" data-update="1" title="Rewrite only ' + esc(stale.projects.join(", ")) + '">' + swap(labels, "Update") + "</button>";
    else html += '<button type="button" class="dm-btn dm-btn-primary dm-btn-sm" data-act="generate"' + (reportable(list).length ? "" : " disabled") + ">" + swap(labels, rep ? "Regenerate" : "Generate") + "</button>";
    html += moreMenu([["Version history" + (rep ? " (" + rep.v + ")" : ""), "unavailable"], ["Export Markdown…", "unavailable"], ["Prompt: standard", "unavailable"]]);
    html += "</div></header>";

    html += '<div class="dm-scroll" data-scroll="report-' + kind + '"><div class="dm-doc">';
    if (job) {
      html += '<div class="dm-empty"><div class="dm-empty-title">' + (job === "stage2" ? "Writing the report…" : "Digesting " + reportable(list).length + " sessions…") + '</div><div class="dm-muted">' + (job === "stage2" ? "Grouping digests by project into an outline with anchors." : "Each session is condensed into a cached digest first.") + '</div><div class="dm-progress"><i class="' + job + '"></i></div></div>';
    } else if (!rep) {
      var n = reportable(list).length;
      var np = byProject(list).length;
      html += '<div class="dm-empty"><div class="dm-empty-title">No report for ' + esc(titleOf(kind, key).split(" · ")[0]) + ' yet</div><div class="dm-muted">' + n + (n === 1 ? " session" : " sessions") + " in " + np + (np === 1 ? " project" : " projects") + ' are ready. Sort them in the Sessions tab, or generate now.</div><div class="dm-actions"><button type="button" class="dm-btn dm-btn-primary dm-btn-sm" data-act="generate">Generate</button><button type="button" class="dm-btn dm-btn-secondary dm-btn-sm" data-act="tab" data-kind="' + kind + '" data-tab="sessions">Review sessions</button></div></div>';
      // Notes can be written before there is a report; they wait here and merge in once it exists.
      html += periodBody(kind, key, [], false, state.source);
    } else if (state.editing) {
      html += '<div class="dm-editor"><textarea class="dm-textarea" spellcheck="false" aria-label="Report Markdown">' + esc(rep.md) + '</textarea><div class="dm-actions"><button type="button" class="dm-btn dm-btn-primary dm-btn-sm" data-act="save">Save as v' + (rep.v + 1) + '</button><button type="button" class="dm-btn dm-btn-ghost dm-btn-sm" data-act="edit" data-on="0">Cancel</button></div></div>';
    } else {
      if (stale) {
        html += '<div class="dm-banner warn"><span>Out of date: ' + stale.count + " session " + (stale.count === 1 ? "change" : "changes") + " since this version · affects " + esc(stale.projects.join(", ")) + '</span><button type="button" class="dm-btn dm-btn-secondary dm-btn-xs" data-act="generate" data-update="1">Update ' + (stale.projects.length === 1 ? "section" : "sections") + "</button></div>";
      }
      html += renderMarkdown(kind, key, rep.md, true, state.source);
    }
    html += "</div></div></section>";
    return html;
  }

  function renderMarkdown(kind, key, md, withActions, source) {
    var humanOnly = source === "human";
    var doc = splitMarkdown(md);
    var expandAll = allExpanded(kind, key);
    var rep = reports[kind] && reports[kind][key];
    var html = '<header class="dm-doc-head"><h3 class="dm-doc-title">' + esc(doc.title) + "</h3>";
    if (withActions) {
      html += '<div class="dm-doc-actions">';
      if (rep) html += '<button type="button" class="dm-status ' + rep.status + '" data-act="status" title="Click to ' + (rep.status === "final" ? "reopen as draft" : "mark final") + '"><span class="dm-sdot ' + rep.status + '"></span>' + "<span>" + statusLabel(rep.status) + " · v" + rep.v + "</span></button>";
      html += '<button type="button" class="dm-btn dm-btn-ghost dm-btn-xs" data-act="copy" title="Copy as Markdown"><svg ' + ICON + '><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg></button>';
      if (hasNested(md) && !humanOnly) html += '<button type="button" class="dm-btn dm-btn-ghost dm-btn-xs" data-act="expand-all" data-kind="' + kind + '" data-key="' + key + '">' + (expandAll ? "Collapse all" : "Expand all") + "</button>";
      html += "</div>";
    }
    html += "</header>";
    var body = periodBody(kind, key, doc.sections, expandAll, source || "both");
    if (humanOnly && !body) body = '<div class="dm-empty-line">Nothing written by you here. In the demo, 2026-11-18 has notes on the day, notes on projects and an annotation.</div>';
    return html + body;
  }

  /* ----------------------------------------------------- render: sessions */

  function sliceRow(s, opts) {
    opts = opts || {};
    var cls = "dm-slice" + (state.drawer && state.drawer.slice === s.id ? " selected" : "") + (s.ignored ? " ignored" : "");
    var html = '<div class="' + cls + '"><button type="button" class="dm-slice-main" data-act="open-slice" data-slice="' + s.id + '">';
    html += '<div class="dm-slice-title">' + (s.ignored ? '<span class="dm-badge">Ignored</span>' : "") + "<span>" + esc(s.title) + "</span></div>";
    html += '<div class="dm-slice-meta">' + agentBadge(s.agent) + "<span>" + (opts.showDate ? s.date.slice(5) + " " : "") + hhmm(s.start) + "-" + hhmm(s.end) + "</span><span>" + s.minutes + "m</span><span>" + s.turns + "T</span></div></button>";
    html += '<div class="dm-slice-ctl">';
    if (opts.controls !== false) {
      if (s.ignored) {
        html += '<button type="button" class="dm-btn dm-btn-secondary dm-btn-xs" data-act="ignore" data-slice="' + s.id + '">Include</button>';
      } else {
        html += '<button type="button" class="dm-btn dm-btn-ghost dm-btn-xs dm-hover" data-act="ignore" data-slice="' + s.id + '">Ignore</button>';
        if (s.projectId === null && s.suggest) html += '<button type="button" class="dm-btn dm-btn-secondary dm-btn-xs" data-act="accept" data-slice="' + s.id + '" title="Suggested by ' + (s.source === "rule" ? "a rule" : "the model") + '">✓ ' + esc(projById(s.suggest).name) + "</button>";
        html += '<select class="dm-select" data-act="assign" data-slice="' + s.id + '" aria-label="Project for ' + esc(s.title) + '"><option value="">Unassigned</option>';
        projects.forEach(function (p) { html += '<option value="' + p.id + '"' + (s.projectId === p.id ? " selected" : "") + ">" + esc(p.name) + "</option>"; });
        html += "</select>";
      }
    }
    return html + "</div></div>";
  }

  function renderSessions(kind, key, tabsHtml) {
    var list = periodSlices(kind, key);
    var active = list.filter(function (s) { return !s.ignored; });
    var ignored = list.filter(function (s) { return s.ignored; });
    var unassigned = active.filter(function (s) { return s.projectId === null; });
    var suggestions = unassigned.filter(function (s) { return s.suggest; });
    var filter = state.filter === "ignored" && !ignored.length ? "all" : state.filter;
    var html = '<section class="dm-panel"><header class="dm-panel-head"><div class="dm-head-left">' + tabsHtml + '</div><div class="dm-head-right"><div class="dm-pills">';
    [["all", "All (" + active.length + ")"], ["unassigned", "Unassigned (" + unassigned.length + ")"], ["ignored", "Ignored (" + ignored.length + ")"]].forEach(function (f) {
      var off = f[0] === "ignored" && !ignored.length;
      html += '<button type="button" class="dm-pill' + (filter === f[0] ? " active" : "") + '" data-act="filter" data-filter="' + f[0] + '"' + (off ? " disabled" : "") + ">" + f[1] + "</button>";
    });
    html += "</div>";
    html += '</div></header><div class="dm-scroll" data-scroll="sessions-' + kind + '"><div class="dm-container">';
    if (suggestions.length) {
      html += '<div class="dm-banner suggest"><span>' + suggestions.length + (suggestions.length === 1 ? " session has" : " sessions have") + ' a suggested project (shown on the row).</span><button type="button" class="dm-btn dm-btn-primary dm-btn-xs" data-act="accept-all" data-kind="' + kind + '">Accept suggestions (' + suggestions.length + ")</button></div>";
    }
    var showDate = kind !== "daily";
    if (filter === "ignored") {
      html += '<div class="dm-group">' + ignored.map(function (s) { return sliceRow(s, { showDate: showDate }); }).join("") + "</div>";
    } else {
      var shown = filter === "unassigned" ? unassigned : active;
      if (!shown.length) html += '<div class="dm-empty-line">' + (filter === "unassigned" ? "Everything is sorted. Nice." : "No sessions.") + "</div>";
      var groups = [{ name: "Unassigned", items: shown.filter(function (s) { return s.projectId === null; }) }];
      projects.forEach(function (p) { groups.push({ name: p.name, items: shown.filter(function (s) { return s.projectId === p.id; }) }); });
      groups.forEach(function (g) {
        if (!g.items.length) return;
        html += '<div class="dm-group"><div class="dm-group-head"><span>' + esc(g.name) + '</span><span class="dm-muted">' + g.items.length + "</span></div>";
        html += g.items.map(function (s) { return sliceRow(s, { showDate: showDate }); }).join("") + "</div>";
      });
    }
    html += '<p class="dm-hint">Tip: pick a project from a row\'s menu, or hover a row to ignore it. The report notices and offers an Update.</p>';
    return html + "</div></div></section>";
  }

  /* ----------------------------------------------------- render: projects */

  function renderProjects() {
    var p = projById(state.projectId);
    var kind = "projects";
    var tab = state.tab.projects;
    var from = rangeStart();
    var inRange = function (d) { return !from || d >= from; };
    var mine = slices.filter(function (s) { return s.projectId === p.id && inRange(s.date); });
    var tabsHtml = tabs(kind, [["report", "Report"], ["sessions", "Sessions (" + notIgnored(mine).length + ")"]]);
    var ruleCount = rules.filter(function (r) { return r.projectId === p.id; }).length;
    var menu = (tab === "report" ? [["Export Markdown…", "unavailable"], "-"] : []).concat([
      ["Edit project details…", "manage", ' data-manage="details"'],
      ["Assignment rules (" + ruleCount + ")…", "manage", ' data-manage="rules"'],
      "-",
      ["Archive project", "unavailable"],
      ["Delete project…", "unavailable", ' class="danger"'],
    ]);

    var html = '<section class="dm-panel"><header class="dm-panel-head"><div class="dm-head-left">' + tabsHtml + '</div><div class="dm-head-right">';
    if (tab === "report") {
      html += sourceFilter(false);
      html += '<div class="dm-pills" role="radiogroup" aria-label="Summarise by">';
      [["daily", "Day", "By day"], ["weekly", "Week", "By week"], ["monthly", "Month", "By month"]].forEach(function (g) {
        html += '<button type="button" role="radio" aria-checked="' + (state.granularity === g[0]) + '" class="dm-pill' + (state.granularity === g[0] ? " active" : "") + '" data-act="granularity" data-g="' + g[0] + '"' + tip(g[2]) + ">" + g[1] + "</button>";
      });
      html += "</div>";
      html += '<button type="button" class="dm-btn dm-btn-ghost dm-icon-btn" data-act="sort"' + tip(state.oldestFirst ? "Oldest first. Click to show newest first." : "Newest first. Click to show oldest first.", "end") + ">" + (state.oldestFirst ? IC.oldest : IC.newest) + "</button>";
    }
    html += rangeSelect() + moreMenu(menu);
    html += '</div></header><div class="dm-scroll" data-scroll="project-' + tab + '"><div class="dm-doc">';

    if (tab === "report") {
      html += '<header class="dm-doc-head"><div><h3 class="dm-doc-title">' + esc(p.name) + '</h3><div class="dm-muted">' + esc(p.desc) + "</div></div></header>";
      var pageNotes = state.source === "agent" ? null : (NOTES.project || {})[p.key];
      if (pageNotes) html += '<div class="dm-hday"><div class="dm-hhead">Project notes</div>' + noteBox(pageNotes, "project|" + p.key) + "</div>";
      var g = state.granularity;
      var keys;
      if (g === "daily") keys = datesWithSessions.filter(function (d) { return mine.some(function (s) { return s.date === d && !s.ignored; }); });
      else if (g === "weekly") keys = Object.keys(weekMondays).filter(function (wk) { return mine.some(function (s) { return weekKey(s.date) === wk; }); });
      else keys = Object.keys(DATA.monthly).filter(function (m) { return !from || m + "-31" >= from; });
      keys = keys.sort();
      if (!state.oldestFirst) keys.reverse();
      var missing = keys.filter(function (k) { return !reports[g][k]; });
      if (missing.length) {
        html += '<div class="dm-banner info"><span>No ' + (g === "daily" ? "daily" : g === "weekly" ? "weekly" : "monthly") + " report yet for:</span><div class=\"dm-chips\">";
        missing.forEach(function (k) { html += '<button type="button" class="dm-chip" data-act="goto" data-kind="' + g + '" data-key="' + k + '">' + (g === "daily" ? k.slice(5) : k) + "</button>"; });
        html += "</div></div>";
      }
      var shownAny = false;
      keys.forEach(function (k) {
        var rep = reports[g][k];
        if (!rep) return;
        var sec = splitMarkdown(rep.md).sections.filter(function (x) { return x.name === p.name; })[0];
        if (!sec) return;
        var pn = state.source === "agent" ? {} : projectNotes(g, k, p.key);
        if (state.source === "human" && !hasNotes(pn)) return;
        shownAny = true;
        var label = g === "daily" ? k + " · " + WD_SHORT[weekday(k)] : g === "weekly" ? weekTitle(k) : monthTitle(k);
        html += '<div class="dm-card"><div class="dm-card-head"><strong>' + esc(label) + "</strong>" + statusDot(rep.status) + '<button type="button" class="dm-btn dm-btn-ghost dm-btn-xs dm-card-act" data-act="goto" data-kind="' + g + '" data-key="' + k + '">Open -&gt;</button></div>';
        html += sectionBody(sec.lines, "proj|" + g + "|" + k, false, pn, state.source, g + "|" + k + "|" + p.key) + "</div>";
      });
      if (state.source === "human" && !shownAny && !pageNotes) html += '<div class="dm-empty-line">Nothing written by you on ' + esc(p.name) + " in this range.</div>";
      return html + "</div></div></section>";
    }

    // Sessions tab: management cards opened from the menu, pending suggestions, then the timeline.
    if (state.manage === "details") {
      html += '<div class="dm-card"><div class="dm-form"><label><span class="dm-muted">Project name</span><input class="dm-input" type="text" value="' + esc(p.name) + '"></label><label><span class="dm-muted">Description</span><input class="dm-input" type="text" value="' + esc(p.desc) + '"></label></div><div class="dm-actions"><button type="button" class="dm-btn dm-btn-primary dm-btn-sm" data-act="close-manage">Save</button><button type="button" class="dm-btn dm-btn-ghost dm-btn-sm" data-act="close-manage">Cancel</button></div></div>';
    } else if (state.manage === "rules") {
      html += '<div class="dm-card"><div class="dm-card-head"><h4>Assignment rules for ' + esc(p.name) + '</h4><button type="button" class="dm-btn dm-btn-ghost dm-btn-xs dm-card-act" data-act="close-manage">Close</button></div><div class="dm-muted dm-rules-note">Rules match a session\'s directory, Git remote or title. Auto rules file sessions directly; the others only suggest.</div>';
      rules.filter(function (r) { return r.projectId === p.id; }).forEach(function (r) {
        html += '<div class="dm-rule"><span class="dm-badge">' + r.kind + '</span><code>' + esc(r.value) + '</code><button type="button" class="dm-status ' + (r.auto ? "final" : "draft") + '" data-act="rule-auto" data-rule="' + r.id + '"><span class="dm-sdot ' + (r.auto ? "final" : "draft") + '"></span>' + (r.auto ? "Auto" : "Suggest") + "</button></div>";
      });
      html += "</div>";
    }
    var sug = slices.filter(function (s) { return s.projectId === null && !s.ignored && s.suggest === p.id; });
    if (sug.length) {
      var SHOWN = 3;
      var shownSug = state.showAllSuggestions ? sug : sug.slice(0, SHOWN);
      html += '<div class="dm-card"><div class="dm-card-head"><h4>Suggested sessions (' + sug.length + ')</h4><button type="button" class="dm-btn dm-btn-primary dm-btn-xs dm-card-act" data-act="accept-project" data-project="' + p.id + '">Accept all (' + sug.length + ")</button></div>";
      shownSug.forEach(function (s) {
        html += '<div class="dm-slice"><button type="button" class="dm-slice-main" data-act="open-slice" data-slice="' + s.id + '"><div class="dm-slice-title"><span>' + esc(s.title) + '</span></div><div class="dm-slice-meta">' + agentBadge(s.agent) + "<span>" + s.date.slice(5) + " " + hhmm(s.start) + "</span><span>suggested by " + (s.source === "rule" ? "rule" : "model") + '</span></div></button><div class="dm-slice-ctl"><button type="button" class="dm-btn dm-btn-secondary dm-btn-xs" data-act="accept" data-slice="' + s.id + '">Accept</button></div></div>';
      });
      if (sug.length > SHOWN) html += '<button type="button" class="dm-btn dm-btn-ghost dm-btn-xs dm-more-sug" data-act="show-all-suggestions">' + (state.showAllSuggestions ? "Show fewer" : "Show all " + sug.length) + "</button>";
      html += "</div>";
    }
    datesWithSessions.slice().reverse().forEach(function (d) {
      var day = mine.filter(function (s) { return s.date === d; });
      if (!day.length) return;
      html += '<div class="dm-group"><div class="dm-group-head"><span>' + d + " · " + WD_SHORT[weekday(d)] + '</span><span class="dm-muted">' + day.length + "</span></div>" + day.map(function (s) { return sliceRow(s, { controls: false }); }).join("") + "</div>";
    });
    if (!mine.length) html += '<div class="dm-empty-line">No sessions assigned to ' + esc(p.name) + " in the selected time range.</div>";
    return html + "</div></div></section>";
  }

  /* -------------------------------------------------------- render: stats */

  function renderStats() {
    var days = [];
    for (var d = SHOWN_MONTH + "-01"; d <= TODAY; d = addDays(d, 1)) days.push(dayOverview(d) || { date: d, slices: 0, minutes: 0 });
    var active = days.filter(function (x) { return x.minutes; });
    var mins = active.reduce(function (a, x) { return a + x.minutes; }, 0);
    var sess = active.reduce(function (a, x) { return a + x.slices; }, 0);
    var streak = 0, run = 0;
    days.forEach(function (x) { run = x.minutes ? run + 1 : 0; streak = Math.max(streak, run); });
    var max = Math.max.apply(null, days.map(function (x) { return x.minutes; }));
    var monthSlices = slices.filter(function (s) { return s.date.slice(0, 7) === SHOWN_MONTH && !s.ignored; });
    var byAgent = {};
    monthSlices.forEach(function (s) { byAgent[s.agent] = (byAgent[s.agent] || 0) + s.minutes; });
    var byProj = {};
    monthSlices.forEach(function (s) { var n = s.projectId ? projById(s.projectId).name : "Unassigned"; byProj[n] = (byProj[n] || 0) + s.minutes; });
    var total = monthSlices.reduce(function (a, s) { return a + s.minutes; }, 0) || 1;

    var html = '<section class="dm-panel"><header class="dm-panel-head"><div class="dm-head-left"><strong class="dm-panel-title">Stats</strong></div><div class="dm-head-right"><div class="dm-pills"><button type="button" class="dm-pill active">This month</button><button type="button" class="dm-pill" data-act="unavailable">Last 90 days</button></div></div></header><div class="dm-scroll"><div class="dm-doc">';
    html += '<div class="dm-kpis">';
    [["Active time", fmtMinutes(mins)], ["Sessions", sess], ["Active days", active.length + " / " + days.length], ["Longest streak", streak + " days"]].forEach(function (k) {
      html += '<div class="dm-kpi"><div class="dm-muted">' + k[0] + '</div><div class="dm-kpi-v">' + k[1] + "</div></div>";
    });
    html += '</div><div class="dm-card"><div class="dm-card-head"><h4>Active time per day</h4><span class="dm-muted dm-card-act">parallel sessions counted once</span></div><div class="dm-bars">';
    days.forEach(function (x) {
      html += '<div class="dm-barcol" title="' + x.date + ": " + fmtMinutes(x.minutes) + '"><i style="height:' + (x.minutes ? Math.max(4, (x.minutes / max) * 100) : 0) + '%"></i><span>' + parseInt(x.date.slice(8), 10) + "</span></div>";
    });
    html += "</div></div>";
    function split(title, map, label) {
      var h = '<div class="dm-card"><div class="dm-card-head"><h4>' + title + "</h4></div>";
      Object.keys(map).sort(function (a, b) { return map[b] - map[a]; }).forEach(function (k) {
        var pct = Math.round((map[k] / total) * 100);
        h += '<div class="dm-split"><span>' + esc(label(k)) + '</span><div class="dm-split-bar"><i style="width:' + pct + '%"></i></div><span class="dm-muted">' + pct + "%</span></div>";
      });
      return h + "</div>";
    }
    html += '<div class="dm-two">' + split("By agent", byAgent, function (k) { return DATA.agents[k]; }) + split("By project", byProj, function (k) { return k; }) + "</div>";
    html += '<div class="dm-card"><div class="dm-card-head"><h4>Tokens this month</h4></div><div class="dm-token-row"><div><div class="dm-kpi-v">41.2M</div><div class="dm-muted">input + output</div></div><div><div class="dm-kpi-v">88%</div><div class="dm-muted">cache hit rate</div></div><div><div class="dm-kpi-v">6</div><div class="dm-muted">models used</div></div></div></div>';
    return html + "</div></div></section>";
  }

  /* --------------------------------------------------------- render: drawer */

  function renderDrawer() {
    var d = state.drawer;
    if (!d) return "";
    var s = sliceById(d.slice);
    if (!s) return "";
    var p = projById(s.projectId);
    var turns = turnsOf(s);
    var html = '<div class="dm-backdrop" data-act="close-drawer"></div><aside class="dm-drawer" aria-label="Session details"><div class="dm-drawer-head"><div><h4>S' + s.id + " · " + esc(s.title) + '</h4><div class="dm-muted">' + agentBadge(s.agent) + " " + s.date + " · " + hhmm(s.start) + "-" + hhmm(s.end) + " · " + s.turns + ' turns</div></div><button type="button" class="dm-btn dm-btn-ghost dm-btn-sm" data-act="close-drawer">Close</button></div>';
    html += '<div class="dm-drawer-body" data-scroll="drawer">';
    html += '<div class="dm-meta"><div><span>Directory:</span> <code>' + esc(s.dir) + "</code></div>" + (s.branch ? "<div><span>Branch:</span> <code>" + esc(s.branch) + "</code></div>" : "") + "<div><span>Project:</span> " + (p ? esc(p.name) : "Unassigned") + "</div></div>";
    html += '<div class="dm-digest"><h5>Slice Digest</h5><div class="dm-muted">Written by opus</div><div><strong>Goal:</strong> ' + esc(s.item.s) + "</div><div><strong>Key changes:</strong><ul><li>" + esc(s.item.p) + ' <button type="button" class="dm-anchor" data-act="turn" data-turn="' + s.k + '">T' + s.k + "</button></li>";
    s.item.d.forEach(function (x, j) { html += "<li>" + esc(x) + ' <button type="button" class="dm-anchor" data-act="turn" data-turn="' + (s.k + 1 + j) + '">T' + (s.k + 1 + j) + "</button></li>"; });
    html += "</ul></div></div><h5>Turns</h5>";
    turns.forEach(function (t) {
      html += '<div class="dm-turn' + (t.idx === d.turn ? " highlighted" : "") + '" data-turn-card="' + t.idx + '"><div class="dm-turn-head"><span class="dm-turn-idx">T' + t.idx + '</span><span class="dm-muted">' + t.time + '</span></div><div class="dm-turn-q">' + esc(t.q) + '</div><div class="dm-turn-a">' + esc(t.a) + "</div>";
      if (t.tools.length) html += '<div class="dm-tools">' + t.tools.map(function (x) { var i = x.indexOf(" "); return '<div><span class="dm-badge">' + esc(x.slice(0, i)) + "</span> <code>" + esc(x.slice(i + 1)) + "</code></div>"; }).join("") + "</div>";
      html += "</div>";
    });
    return html + "</div></aside>";
  }

  /* -------------------------------------------------------------- render */

  function render() {
    // Remember which report kind was open, so "Open Project Report" picks the matching granularity.
    if (state.mode !== "projects" && !state.stats) state.lastPeriodKind = state.mode;
    var scrolls = {};
    root.querySelectorAll("[data-scroll]").forEach(function (el) { scrolls[el.getAttribute("data-scroll")] = el.scrollTop; });
    var main = state.stats ? renderStats() : state.mode === "projects" ? renderProjects() : renderPeriod(state.mode);
    root.innerHTML =
      '<div class="dm-titlebar"><div class="dm-lights"><i></i><i></i><i></i></div><div class="dm-wtitle"></div><span class="dm-demo-tag">Demo · made-up data</span></div>' +
      '<div class="dm-shell">' + renderSidebar() + '<main class="dm-main">' + main + "</main>" + renderDrawer() +
      (state.toast ? '<div class="dm-toast" role="status">' + esc(state.toast) + "</div>" : "") + "</div>";
    root.querySelectorAll("[data-scroll]").forEach(function (el) {
      var k = el.getAttribute("data-scroll");
      if (k in scrolls) el.scrollTop = scrolls[k];
    });
  }

  function scrollDrawerToTurn() {
    var card = root.querySelector('.dm-turn.highlighted');
    var body = root.querySelector(".dm-drawer-body");
    if (card && body) body.scrollTop = card.offsetTop - body.offsetTop - 12;
  }

  var toastTimer = null;
  function toast(msg) {
    state.toast = msg;
    render();
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { state.toast = null; render(); }, 2200);
  }

  /* ------------------------------------------------------------- actions */

  function generate(kind, key, update, finished) {
    var jobKey = kind + "|" + key;
    if (state.jobs[jobKey]) return;
    state.editing = false;
    state.jobs[jobKey] = "stage1";
    render();
    setTimeout(function () {
      if (!state.jobs[jobKey]) { if (finished) finished(false); return; }
      state.jobs[jobKey] = "stage2";
      render();
      setTimeout(function () {
        if (!state.jobs[jobKey]) { if (finished) finished(false); return; }
        delete state.jobs[jobKey];
        var old = reports[kind][key];
        var list = periodSlices(kind, key);
        var fresh = buildSections(kind, key, list);
        var md;
        if (update && old) {
          var st = staleness(kind, key);
          var affected = st ? st.projects : [];
          var doc = splitMarkdown(old.md);
          var kept = doc.sections.filter(function (sec) { return affected.indexOf(sec.name) < 0; });
          var added = fresh.filter(function (sec) { return affected.indexOf(sec.name) >= 0; });
          var merged = kept.concat(added).sort(function (a, b) { return projOrder(a.name) - projOrder(b.name); });
          md = serialize(doc.title || titleOf(kind, key), merged);
        } else {
          md = serialize(titleOf(kind, key), fresh);
        }
        reports[kind][key] = { md: md, v: old ? old.v + 1 : 1, status: "draft", manifest: reportManifest(kind, key, list) };
        if (finished) finished(true);
        render();
      }, 1500);
    }, 1100);
  }

  function batchSelection(kind) {
    return state.batches[kind] || (state.batches[kind] = { keys: [], includeFinal: false });
  }

  function listKeys() {
    return Array.from(root.querySelectorAll("[data-list-key]"), function (el) {
      return el.getAttribute("data-list-key");
    });
  }

  function selectBatchRow(kind, key, range) {
    var batch = batchSelection(kind);
    if (batch.running) return;
    batch.open = true;
    var keys = listKeys();
    var start = keys.indexOf(batch.anchor || periodKey(kind));
    var end = keys.indexOf(key);
    if (range && start >= 0) {
      batch.keys = Array.from(new Set(batch.keys.concat(keys.slice(Math.min(start, end), Math.max(start, end) + 1))));
    } else {
      batch.keys = batch.keys.indexOf(key) < 0 ? batch.keys.concat(key) : batch.keys.filter(function (k) { return k !== key; });
      batch.anchor = key;
    }
    batch.summary = null;
    batch.confirm = false;
  }

  function runBatch(kind, mode) {
    var batch = state.batches[kind];
    if (mode === "regenerate") { batch.confirm = true; return; }
    batch.confirm = false;
    var plan = batch.keys.map(function (key) { return batchItem(kind, key, batch.includeFinal); });
    var queue = plan.filter(function (p) {
      if (state.jobs[kind + "|" + p.key]) return false;
      if (mode === "confirmed") return p.eligible;
      return (mode !== "missing" && p.stale) || (mode !== "stale" && p.missing);
    });
    var skipped = plan.length - queue.length;
    var job = batch.running = { done: 0, total: queue.length, active: [] };
    var completed = 0;
    function next() {
      var p = queue.shift();
      if (!p || job.cancelled) return;
      job.active.push(p.key);
      generate(kind, p.key, p.stale && mode !== "confirmed", function (ok) {
        job.active = job.active.filter(function (key) { return key !== p.key; });
        job.done += 1;
        if (ok) completed += 1;
        if (!job.cancelled && queue.length) next();
        if (job.active.length === 0) finish();
      });
    }
    function finish() {
      var parts = [completed + " completed"];
      if (skipped) parts.push(skipped + " skipped");
      if (job.cancelled) parts.push(job.total - completed + " cancelled");
      batch.running = null;
      batch.keys = kind === "monthly" ? plan.filter(function (p) { return !p.available; })
        .map(function (p) { return p.key; }) : [];
      batch.summary = parts.join(" · ");
      render();
    }
    if (!queue.length) finish();
    else { next(); next(); }
  }

  function openPeriod(kind, key) {
    state.stats = false;
    state.mode = kind;
    state.editing = false;
    state.menu = null;
    if (kind === "daily") state.date = key;
    else if (kind === "weekly") state.week = key;
    else state.month = key;
    state.tab[kind] = "report";
  }

  root.addEventListener("click", function (ev) {
    var el = ev.target.closest("[data-act]");
    if (!el || !root.contains(el) || el.tagName === "SELECT") return;
    var act = el.getAttribute("data-act");
    var kind, key, s;
    var listKey = el.getAttribute("data-list-key");
    if (listKey && (ev.metaKey || ev.ctrlKey || ev.shiftKey)) {
      selectBatchRow(state.mode, listKey, ev.shiftKey && !(ev.metaKey || ev.ctrlKey));
      render();
      return;
    }
    if (act !== "menu" && state.menu) state.menu = null;
    switch (act) {
      case "batch-select":
        var pick = el.getAttribute("data-pick");
        var selection = batchSelection(state.mode);
        if (selection.running) break;
        if (pick === "select") {
          if (selection.open) state.batches[state.mode] = null;
          else selection.open = true;
          break;
        }
        selection.open = true;
        selection.summary = null;
        selection.confirm = false;
        selection.keys = listKeys().filter(function (period) {
          var rep = reports[state.mode][period];
          if (pick === "drafts") return rep && rep.status === "draft";
          if (pick === "stale") return batchItem(state.mode, period, true).stale;
          if (pick === "week") return weekKey(period) === CUR_WEEK;
          if (pick === "month") return period.slice(0, 7) === CUR_MONTH;
          return !rep;
        });
        break;
      case "batch-toggle": selectBatchRow(state.mode, el.getAttribute("data-key"), false); break;
      case "batch-close": state.batches[state.mode] = null; break;
      case "batch-final": state.batches[state.mode].includeFinal = !state.batches[state.mode].includeFinal; break;
      case "batch-dismiss": state.batches[state.mode].confirm = false; break;
      case "batch-run": runBatch(state.mode, el.getAttribute("data-value")); break;
      case "batch-cancel":
        var running = state.batches[state.mode].running;
        running.cancelled = true;
        running.active.forEach(function (key) { delete state.jobs[state.mode + "|" + key]; });
        break;
      case "batch-status":
        var batch = state.batches[state.mode];
        var marked = batch.keys.filter(function (key) {
          var rep = reports[state.mode][key];
          if (rep) rep.status = el.getAttribute("data-value");
          return !!rep;
        }).length;
        batch.summary = marked + " marked " + el.getAttribute("data-value");
        break;
      case "mode":
        state.mode = el.getAttribute("data-mode");
        state.stats = false;
        state.editing = false;
        break;
      case "day": openPeriod("daily", el.getAttribute("data-date")); break;
      case "week": openPeriod("weekly", el.getAttribute("data-week")); break;
      case "month": openPeriod("monthly", el.getAttribute("data-month")); break;
      case "jump":
        if (state.mode === "daily") state.date = TODAY;
        else if (state.mode === "weekly") state.week = CUR_WEEK;
        else state.month = CUR_MONTH;
        break;
      case "project":
        state.projectId = parseInt(el.getAttribute("data-project"), 10);
        state.stats = false;
        state.manage = null;
        state.showAllSuggestions = false;
        break;
      case "tab":
        state.tab[el.getAttribute("data-kind")] = el.getAttribute("data-tab");
        state.editing = false;
        break;
      case "toggle":
        key = el.getAttribute("data-id");
        state.expanded[key] = !state.expanded[key];
        break;
      case "note-fold":
        key = el.getAttribute("data-id");
        state.foldedNotes[key] = !state.foldedNotes[key];
        break;
      case "expand-all":
        key = "all|" + el.getAttribute("data-kind") + "|" + el.getAttribute("data-key");
        var on = !state.expanded[key];
        state.expanded[key] = on;
        if (!on) Object.keys(state.expanded).forEach(function (k) { if (k.indexOf(el.getAttribute("data-kind") + "|" + el.getAttribute("data-key") + "|") === 0) delete state.expanded[k]; });
        break;
      case "anchor":
        state.drawer = { slice: parseInt(el.getAttribute("data-slice"), 10), turn: parseInt(el.getAttribute("data-turn"), 10) };
        render();
        scrollDrawerToTurn();
        return;
      case "open-slice":
        s = sliceById(parseInt(el.getAttribute("data-slice"), 10));
        state.drawer = { slice: s.id, turn: s.k };
        render();
        scrollDrawerToTurn();
        return;
      case "turn":
        state.drawer.turn = parseInt(el.getAttribute("data-turn"), 10);
        render();
        scrollDrawerToTurn();
        return;
      case "close-drawer": state.drawer = null; break;
      case "status":
        kind = state.mode;
        var rep = reports[kind][periodKey(kind)];
        if (rep) rep.status = rep.status === "final" ? "draft" : "final";
        break;
      case "generate":
        generate(state.mode, periodKey(state.mode), el.getAttribute("data-update") === "1");
        return;
      case "cancel":
        delete state.jobs[state.mode + "|" + periodKey(state.mode)];
        break;
      case "edit": state.editing = el.getAttribute("data-on") === "1"; break;
      case "save":
        kind = state.mode;
        var r = reports[kind][periodKey(kind)];
        var ta = root.querySelector(".dm-textarea");
        if (r && ta) { r.md = ta.value; r.v += 1; }
        state.editing = false;
        break;
      case "menu":
        var menu = el.getAttribute("data-menu") || el.getAttribute("data-value") || "options";
        state.menu = state.menu === menu ? null : menu;
        break;
      case "copy":
        kind = state.mode;
        var cr = reports[kind][periodKey(kind)];
        if (cr && navigator.clipboard) navigator.clipboard.writeText(cr.md).then(function () { toast("Report copied as Markdown"); }, function () {});
        return;
      case "filter": state.filter = el.getAttribute("data-filter"); break;
      case "ignore":
        s = sliceById(parseInt(el.getAttribute("data-slice"), 10));
        s.ignored = !s.ignored;
        break;
      case "accept":
        s = sliceById(parseInt(el.getAttribute("data-slice"), 10));
        s.projectId = s.suggest;
        break;
      case "accept-all":
        periodSlices(el.getAttribute("data-kind"), periodKey(el.getAttribute("data-kind"))).forEach(function (x) {
          if (x.projectId === null && x.suggest && !x.ignored) x.projectId = x.suggest;
        });
        break;
      case "open-project":
        state.projectId = parseInt(el.getAttribute("data-project"), 10);
        state.mode = "projects";
        state.tab.projects = "report";
        state.granularity = state.lastPeriodKind || "daily";
        break;
      case "granularity": state.granularity = el.getAttribute("data-g"); break;
      case "source": state.source = el.getAttribute("data-source"); break;
      case "sort": state.oldestFirst = !state.oldestFirst; break;
      case "manage":
        state.manage = el.getAttribute("data-manage");
        state.tab.projects = "sessions";
        break;
      case "close-manage": state.manage = null; break;
      case "show-all-suggestions": state.showAllSuggestions = !state.showAllSuggestions; break;
      case "accept-project":
        key = parseInt(el.getAttribute("data-project"), 10);
        slices.forEach(function (x) { if (x.projectId === null && !x.ignored && x.suggest === key) x.projectId = key; });
        break;
      case "goto": openPeriod(el.getAttribute("data-kind"), el.getAttribute("data-key")); break;
      case "rule-auto":
        rules.forEach(function (x) { if (x.id === parseInt(el.getAttribute("data-rule"), 10)) x.auto = !x.auto; });
        break;
      case "stats": state.stats = !state.stats; break;
      case "scan":
        state.scanning = true;
        state.scanNote = null;
        render();
        setTimeout(function () { state.scanning = false; state.scanNote = "Up to date · 0 new sessions · 0.4 s"; render(); }, 900);
        return;
      case "unavailable":
        // Not part of the demo: no effect beyond closing an open menu.
        break;
    }
    render();
  });

  root.addEventListener("change", function (ev) {
    var el = ev.target;
    if (el.getAttribute("data-act") === "range") {
      state.range = el.value;
      render();
      return;
    }
    if (el.getAttribute("data-act") !== "assign") return;
    var s = sliceById(parseInt(el.getAttribute("data-slice"), 10));
    s.projectId = el.value ? parseInt(el.value, 10) : null;
    render();
  });

  root.addEventListener("keydown", function (ev) {
    if (ev.key !== "Escape") return;
    if (state.drawer || state.menu) { state.drawer = null; state.menu = null; render(); }
    else if (state.batches[state.mode] && !state.batches[state.mode].running &&
             !ev.target.matches("input[type=text], textarea")) {
      state.batches[state.mode] = null;
      render();
    }
  });

  render();
})();
