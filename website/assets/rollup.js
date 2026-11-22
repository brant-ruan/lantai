// Draws the roll-up figure into every [data-rollup] element: an abstract picture of scattered
// sessions (bottom) drifting up along threads into days, days into weeks, and weeks into one
// month (top). The shape narrows as it rises and the colors merge into the accent. Beside some
// days and weeks sits a purple note: what the person wrote, joining the record from the side and
// keeping its own color all the way up. The made-up month comes from a fixed seed, so the picture is the same on every visit; it is redrawn at
// the container's size whenever the width changes.
(function () {
  var NS = "http://www.w3.org/2000/svg";
  var REDUCE = window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches;
  var HUES = 4;
  var uid = 0;

  function rng(seed) {
    return function () {
      seed = (seed + 0x6d2b79f5) | 0;
      var t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  // Four weeks of seven days; weekends are mostly quiet.
  var DATA = (function () {
    var r = rng(20261102);
    var gauss = function () { return (r() + r() + r() - 1.5) * 2; };
    var days = [], sessions = [], dust = [];
    for (var i = 0; i < 28; i++) {
      var weekend = i % 7 >= 5;
      var day = { i: i, week: Math.floor(i / 7), mins: 0 };
      if (r() < (weekend ? 0.3 : 0.92)) {
        var n = weekend ? 1 + Math.floor(r() * 3) : 2 + Math.floor(r() * 6);
        for (var k = 0; k < n; k++) {
          var s = { id: sessions.length, day: i, week: day.week, mins: 8 + Math.round(r() * r() * 170), hue: Math.floor(r() * HUES), jx: gauss(), jy: r(), tw: r() };
          sessions.push(s); day.mins += s.mins;
        }
      }
      days.push(day);
    }
    for (var j = 0; j < 90; j++) dust.push({ x: r(), y: r(), size: r(), hue: Math.floor(r() * HUES), tw: r() });
    var weeks = [0, 1, 2, 3].map(function (w) {
      return { w: w, mins: days.slice(w * 7, w * 7 + 7).reduce(function (a, d) { return a + d.mins; }, 0) };
    });
    // Notes the person wrote: on a handful of days (including a quiet one) and on two weeks.
    var notes = [3, 8, 12, 16, 19, 25].map(function (i, k) { return { day: i, week: Math.floor(i / 7), side: k % 2 ? -1 : 1 }; });
    notes.forEach(function (n) { days[n.day].note = true; });
    var weekNotes = [1, 3].map(function (w) { return { week: w }; });
    return { days: days, sessions: sessions, weeks: weeks, dust: dust, notes: notes, weekNotes: weekNotes };
  })();

  function el(name, attrs, parent) {
    var node = document.createElementNS(NS, name);
    for (var k in attrs) node.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(node);
    return node;
  }
  function f(n) { return n.toFixed(1); }
  function curve(x1, y1, x2, y2) {
    var my = (y1 + y2) / 2;
    return "M" + f(x1) + " " + f(y1) + "C" + f(x1) + " " + f(my) + " " + f(x2) + " " + f(my) + " " + f(x2) + " " + f(y2);
  }
  function stops(grad, list) {
    list.forEach(function (s) { el("stop", { offset: s[0], style: "stop-color:" + s[1] + ";stop-opacity:" + s[2] }, grad); });
  }

  function draw(box) {
    var W = box.clientWidth, H = box.clientHeight;
    if (W < 200 || H < 200) return null;
    var id = "ru" + ++uid;
    var x0 = 4, x1 = W - 4, cx = (x0 + x1) / 2, span = x1 - x0;
    var yM = H * 0.08, yW = H * 0.28, yD = H * 0.5, band0 = H * 0.62, band1 = H - 8;
    // Each layer is narrower than the one below it, so the picture reads as a funnel.
    var dayHalf = span * 0.4, unit = (2 * dayHalf) / 31;
    var dayX = function (i) { return cx - dayHalf + unit * (i + Math.floor(i / 7) + 0.5); };
    var weekX = function (w) { return cx + ((dayX(w * 7) + dayX(w * 7 + 6)) / 2 - cx) * 0.62; };
    var clampX = function (x) { return Math.max(x0 + 4, Math.min(x1 - 4, x)); };

    var svg = el("svg", { viewBox: "0 0 " + W + " " + H, width: W, height: H, "aria-hidden": "true" });
    var defs = el("defs", {}, svg);
    for (var h = 0; h < HUES; h++) {
      var lg = el("linearGradient", { id: id + "-t" + h, gradientUnits: "userSpaceOnUse", x1: 0, y1: band1, x2: 0, y2: yD }, defs);
      stops(lg, [[0, "var(--ru-" + h + ")", 0], [0.5, "var(--ru-" + h + ")", 0.28], [1, "var(--accent)", 0.5]]);
    }
    var up = el("linearGradient", { id: id + "-up", gradientUnits: "userSpaceOnUse", x1: 0, y1: yD, x2: 0, y2: yM }, defs);
    stops(up, [[0, "var(--accent)", 0.35], [1, "var(--accent)", 0.12]]);
    var glow = el("radialGradient", { id: id + "-glow" }, defs);
    stops(glow, [[0, "var(--accent)", 0.55], [0.45, "var(--accent)", 0.16], [1, "var(--accent)", 0]]);
    var hglow = el("radialGradient", { id: id + "-hglow" }, defs);
    stops(hglow, [[0, "var(--human)", 0.5], [0.5, "var(--human)", 0.14], [1, "var(--human)", 0]]);

    var gDust = el("g", {}, svg);
    var gLinks = el("g", {}, svg);
    var gNodes = el("g", {}, svg);
    var gFlow = el("g", {}, svg);
    var parts = { svg: svg, flow: gFlow, sessionPath: [], dayPath: [], weekPath: [], notePath: [], weekNotePath: [], all: [], month: null, accent: id };
    var tag = function (node, keys, delay) {
      node.ru = keys;
      node.style.setProperty("--d", Math.round(delay) + "ms");
      parts.all.push(node);
      return node;
    };

    // Dust: faint specks with no thread, the noise around the work.
    DATA.dust.forEach(function (d, i) {
      var c = el("circle", { cx: f(x0 + d.x * span), cy: f(H * 0.36 + d.y * (band1 - H * 0.36)), r: f(0.6 + d.size * 1.1), "class": "ru-el ru-dust ru-c" + d.hue }, gDust);
      c.style.setProperty("--tw", (3 + d.tw * 5).toFixed(1) + "s");
      tag(c, {}, (i * 53) % 900);
    });

    // Threads, drawn under the nodes.
    DATA.weeks.forEach(function (wk) {
      var p = el("path", { d: curve(weekX(wk.w), yW, cx, yM), "class": "ru-el ru-link", stroke: "url(#" + id + "-up)", "stroke-width": f(1.5 + Math.sqrt(wk.mins) / 6), pathLength: 1 }, gLinks);
      parts.weekPath[wk.w] = tag(p, { w: wk.w }, 1900 + wk.w * 90);
    });
    DATA.days.forEach(function (d) {
      if (!d.mins && !d.note) return;
      var p = el("path", { d: curve(dayX(d.i), yD, weekX(d.week), yW), "class": "ru-el ru-link", stroke: "url(#" + id + "-up)", "stroke-width": f(0.8 + Math.sqrt(d.mins) / 7), pathLength: 1 }, gLinks);
      parts.dayPath[d.i] = tag(p, { w: d.week, d: d.i }, 1300 + d.i * 20);
    });
    DATA.sessions.forEach(function (s) {
      s.x = clampX(cx + (dayX(s.day) - cx) * 1.3 + s.jx * unit * 4);
      s.y = band0 + s.jy * (band1 - band0);
      var p = el("path", { d: curve(s.x, s.y, dayX(s.day), yD), "class": "ru-el ru-link", stroke: "url(#" + id + "-t" + s.hue + ")", "stroke-width": 0.9, pathLength: 1 }, gLinks);
      parts.sessionPath[s.id] = tag(p, { w: s.week, d: s.day, s: s.id }, 500 + s.day * 24 + s.jy * 160);
    });

    // The person's notes: a short purple thread into a day (or a week) from just beside it.
    var noteY = yD + (band0 - yD) * 0.42, weekNoteY = yW + (yD - yW) * 0.42;
    DATA.notes.forEach(function (n, k) {
      n.x = clampX(dayX(n.day) + n.side * unit * 1.6);
      n.y = noteY + (k % 3) * 6;
      var p = el("path", { d: curve(n.x, n.y, dayX(n.day), yD), "class": "ru-el ru-link ru-hlink", "stroke-width": 1.4, pathLength: 1 }, gLinks);
      parts.notePath[k] = tag(p, { w: n.week, d: n.day }, 1250 + n.day * 20);
    });
    DATA.weekNotes.forEach(function (n, k) {
      n.x = clampX(weekX(n.week) + (k % 2 ? -1 : 1) * unit * 3.2);
      n.y = weekNoteY;
      var p = el("path", { d: curve(n.x, n.y, weekX(n.week), yW), "class": "ru-el ru-link ru-hlink", "stroke-width": 1.7, pathLength: 1 }, gLinks);
      parts.weekNotePath[k] = tag(p, { w: n.week }, 1850 + n.week * 90);
    });

    // Nodes: sessions are loose specks, days small orbs, weeks larger orbs with a halo.
    DATA.sessions.forEach(function (s) {
      var c = el("circle", { cx: f(s.x), cy: f(s.y), r: f(1.6 + Math.sqrt(s.mins) / 5), "class": "ru-el ru-session ru-c" + s.hue }, gNodes);
      c.style.setProperty("--tw", (2.5 + s.tw * 4).toFixed(1) + "s");
      tag(c, { w: s.week, d: s.day, s: s.id }, (s.id * 37) % 600);
    });
    DATA.days.forEach(function (d) {
      if (!d.mins && !d.note) return;
      var r = 2.4 + Math.sqrt(d.mins) / 8;
      var g = el("g", { "class": "ru-el ru-orb ru-day", transform: "translate(" + f(dayX(d.i)) + " " + f(yD) + ")" }, gNodes);
      el("circle", { r: f(r * 2.4), fill: "url(#" + id + "-glow)" }, g);
      el("circle", { r: f(r), "class": "ru-core" }, g);
      tag(g, { w: d.week, d: d.i }, 1150 + d.i * 20);
    });
    DATA.weeks.forEach(function (wk) {
      var r = 4 + Math.sqrt(wk.mins) / 9;
      var g = el("g", { "class": "ru-el ru-orb ru-week", transform: "translate(" + f(weekX(wk.w)) + " " + f(yW) + ")" }, gNodes);
      el("circle", { r: f(r * 3.4), fill: "url(#" + id + "-glow)" }, g);
      el("circle", { r: f(r + 4), "class": "ru-ring" }, g);
      el("circle", { r: f(r), "class": "ru-core" }, g);
      tag(g, { w: wk.w }, 1750 + wk.w * 90);
    });
    DATA.notes.concat(DATA.weekNotes).forEach(function (n, k) {
      var isDay = n.day != null;
      var g = el("g", { "class": "ru-el ru-orb ru-note", transform: "translate(" + f(n.x) + " " + f(n.y) + ")" }, gNodes);
      el("circle", { r: isDay ? 9 : 12, fill: "url(#" + id + "-hglow)" }, g);
      el("circle", { r: isDay ? 3.2 : 4, "class": "ru-human" }, g);
      tag(g, isDay ? { w: n.week, d: n.day } : { w: n.week }, (isDay ? 1200 + n.day * 20 : 1800 + n.week * 90));
    });
    var mg = el("g", { "class": "ru-el ru-orb ru-month", transform: "translate(" + f(cx) + " " + f(yM) + ")" }, gNodes);
    el("circle", { r: 46, fill: "url(#" + id + "-glow)", "class": "ru-breathe" }, mg);
    el("circle", { r: 18, "class": "ru-ring ru-ring-2" }, mg);
    el("circle", { r: 12, "class": "ru-ring" }, mg);
    el("circle", { r: 7.5, "class": "ru-core" }, mg);
    tag(mg, { m: 1 }, 2300);
    parts.month = { x: cx, y: yM, g: gFlow };
    return parts;
  }

  // Hover lights up the branch under the pointer and dims everything else.
  function related(a, b) {
    if (a.m || b.m) return true;
    if (a.s != null) return b.s != null ? a.s === b.s : b.d != null ? b.d === a.d : b.w === a.w;
    if (a.d != null) return b.d != null ? b.d === a.d : b.w === a.w;
    return b.w === a.w;
  }
  function bindHover(parts) {
    parts.all.forEach(function (node) {
      if (!node.ru || (node.ru.w == null && !node.ru.m) || node.tagName === "path") return;
      node.addEventListener("mouseenter", function () {
        if (node.ru.m) return;
        parts.svg.classList.add("ru-focus");
        parts.all.forEach(function (o) { o.classList.toggle("on", !!o.ru && (o.ru.m || o.ru.w != null) && related(node.ru, o.ru)); });
      });
      node.addEventListener("mouseleave", function () { parts.svg.classList.remove("ru-focus"); });
    });
  }

  // A steady trickle of sparks travelling session -> day -> week -> month; each one takes the
  // accent color once it reaches a day, and sends a ripple out of the month when it arrives.
  function startFlow(parts, state) {
    var sparks = [], last = 0, lastRipple = 0;
    function spawn() {
      var r = Math.random(), chain, c, human = false;
      if (r < 0.1) {
        // A spark from one of the person's notes stays purple on its way up.
        var k = Math.floor(Math.random() * DATA.notes.length), n = DATA.notes[k];
        chain = [parts.notePath[k], parts.dayPath[n.day], parts.weekPath[n.week]];
        human = true;
      } else if (r < 0.14) {
        var j = Math.floor(Math.random() * DATA.weekNotes.length);
        chain = [parts.weekNotePath[j], parts.weekPath[DATA.weekNotes[j].week]];
        human = true;
      } else {
        var s = DATA.sessions[Math.floor(Math.random() * DATA.sessions.length)];
        chain = [parts.sessionPath[s.id], parts.dayPath[s.day], parts.weekPath[s.week]];
      }
      c = el("circle", { r: 1.6, "class": human ? "ru-spark ru-human" : "ru-spark ru-c" + s.hue }, parts.flow);
      sparks.push({ c: c, chain: chain, seg: 0, at: 0, len: chain[0].getTotalLength(), human: human });
    }
    function ripple(t) {
      if (t - lastRipple < 700) return;
      lastRipple = t;
      var c = el("circle", { cx: f(parts.month.x), cy: f(parts.month.y), r: 9, "class": "ru-ripple" }, parts.flow);
      setTimeout(function () { c.remove(); }, 2400);
    }
    function tick(t) {
      if (!state.visible || !parts.svg.isConnected) { state.raf = 0; return; }
      var dt = last ? Math.min(64, t - last) : 16;
      last = t;
      if (sparks.length < 30 && Math.random() < dt / 200) spawn();
      for (var i = sparks.length - 1; i >= 0; i--) {
        var p = sparks[i];
        p.at += dt * 0.06 * (1 + p.seg * 0.4);
        while (p.at >= p.len && p.seg < p.chain.length - 1) {
          p.at -= p.len; p.seg++; p.len = p.chain[p.seg].getTotalLength();
          if (!p.human) p.c.setAttribute("class", "ru-spark ru-spark-up");
          p.c.setAttribute("r", String(1.6 + p.seg * 0.6));
        }
        if (p.at >= p.len) { p.c.remove(); sparks.splice(i, 1); ripple(t); continue; }
        var pt = p.chain[p.seg].getPointAtLength(p.at);
        p.c.setAttribute("cx", f(pt.x));
        p.c.setAttribute("cy", f(pt.y));
      }
      state.raf = requestAnimationFrame(tick);
    }
    state.resume = function () { if (!state.raf) { last = 0; state.raf = requestAnimationFrame(tick); } };
    state.resume();
  }

  function mount(box) {
    var state = { visible: false, played: false, raf: 0, resume: null, width: 0 };
    var parts = null;

    function render() {
      if (parts) parts.svg.remove();
      if (state.raf) cancelAnimationFrame(state.raf);
      state.raf = 0;
      state.resume = null;
      parts = draw(box);
      if (!parts) return;
      box.appendChild(parts.svg);
      bindHover(parts);
      if (state.played) {
        parts.svg.classList.add("ru-play", "ru-done");
        if (!REDUCE) startFlow(parts, state);
      }
    }
    function play() {
      if (state.played || !parts) return;
      state.played = true;
      parts.svg.classList.add("ru-play");
      setTimeout(function () {
        if (!parts) return;
        parts.svg.classList.add("ru-done");
        if (!REDUCE) startFlow(parts, state);
      }, REDUCE ? 0 : 2900);
    }

    render();
    state.width = box.clientWidth;
    if ("ResizeObserver" in window) {
      new ResizeObserver(function () {
        if (Math.abs(box.clientWidth - state.width) < 2) return;
        state.width = box.clientWidth;
        render();
      }).observe(box);
    }
    if ("IntersectionObserver" in window) {
      new IntersectionObserver(function (entries) {
        state.visible = entries[0].isIntersecting;
        if (state.visible) { play(); if (state.resume) state.resume(); }
      }, { threshold: 0.2 }).observe(box);
    } else {
      state.visible = true;
      play();
    }
  }

  function init() { Array.prototype.forEach.call(document.querySelectorAll("[data-rollup]"), mount); }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();
})();
