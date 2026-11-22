// Lantai website behavior: theme toggle, sticky header border, reveal on scroll, copy buttons,
// the two-hands illustration, agents marquee.
(function () {
  var root = document.documentElement;
  root.classList.add("js");
  var ZH = /^zh/i.test(root.lang || "");
  var T = ZH
    ? { light: "浅色主题", dark: "深色主题", system: "跟随系统", change: "（点击切换）", copy: "复制", copied: "已复制" }
    : { light: "Light theme", dark: "Dark theme", system: "System theme", change: " (click to change)", copy: "Copy", copied: "Copied" };

  // Theme: cycles system -> light -> dark; remembered per browser when storage is available.
  var KEY = "lantai-theme";
  function readTheme() {
    try { return localStorage.getItem(KEY); } catch (e) { return null; }
  }
  function applyTheme(theme) {
    if (theme === "light" || theme === "dark") root.setAttribute("data-theme", theme);
    else root.removeAttribute("data-theme");
    var btn = document.getElementById("theme-toggle");
    if (btn) {
      var label = theme === "light" ? T.light : theme === "dark" ? T.dark : T.system;
      btn.setAttribute("aria-label", label + T.change);
      btn.setAttribute("title", label);
      btn.dataset.mode = theme || "system";
    }
  }
  applyTheme(readTheme());

  // Keeps the last few characters of a Chinese paragraph on one line, so a paragraph never
  // ends with a lonely one- or two-character line at any width. (`text-wrap: pretty` only
  // guarantees two characters for CJK text.) English text is left to `text-wrap: pretty`.
  var CJK = /[\u3400-\u9fff\uff00-\uffef\u3000-\u303f]/;
  var TAIL = 5;
  function glueTail(el) {
    var walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    var last = null;
    for (var n = walker.nextNode(); n; n = walker.nextNode()) if (/\S/.test(n.data)) last = n;
    if (!last || !CJK.test(last.data)) return;
    var text = last.data.replace(/\s+$/, "");
    var start = text.length, count = 0;
    while (start > 0 && count < TAIL) { start--; if (!/\s/.test(text[start])) count++; }
    if (start < 2) return; // Too short to split: gluing the whole node could overflow.
    var tail = last.splitText(start);
    var span = document.createElement("span");
    span.className = "nowrap";
    tail.parentNode.replaceChild(span, tail);
    span.appendChild(tail);
  }

  document.addEventListener("DOMContentLoaded", function () {
    applyTheme(readTheme());

    document.querySelectorAll("main p, main li, .requirements > div, .site-footer p").forEach(function (el) {
      if (!el.closest(".lt-demo")) glueTail(el);
    });

    var toggle = document.getElementById("theme-toggle");
    if (toggle) {
      toggle.addEventListener("click", function () {
        var order = ["system", "light", "dark"];
        var next = order[(order.indexOf(toggle.dataset.mode || "system") + 1) % order.length];
        try {
          if (next === "system") localStorage.removeItem(KEY);
          else localStorage.setItem(KEY, next);
        } catch (e) { /* storage unavailable: theme applies for this page view only */ }
        applyTheme(next === "system" ? null : next);
      });
    }

    var header = document.querySelector(".site-header");
    function onScroll() {
      if (header) header.classList.toggle("scrolled", window.scrollY > 8);
    }
    window.addEventListener("scroll", onScroll, { passive: true });
    onScroll();

    var items = document.querySelectorAll(".reveal");
    if ("IntersectionObserver" in window) {
      var io = new IntersectionObserver(function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add("in");
            io.unobserve(entry.target);
          }
        });
      }, { rootMargin: "0px 0px -8% 0px" });
      items.forEach(function (el) { io.observe(el); });
    } else {
      items.forEach(function (el) { el.classList.add("in"); });
    }

    document.querySelectorAll(".copy").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var target = document.getElementById(btn.dataset.target);
        if (!target) return;
        var text = Array.prototype.map.call(target.querySelectorAll(".cmd"), function (line) {
          return line.textContent;
        }).join("\n");
        var done = function () {
          btn.textContent = T.copied;
          setTimeout(function () { btn.textContent = T.copy; }, 1600);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done, function () {});
        }
      });
    });

    // Two-hands illustration: inlined so its colors follow the page theme (an <img> would not
    // see the theme toggle). It stays static until it scrolls into view, then the agent's lines
    // appear and the person's strokes write themselves. The <img> stays if the fetch fails.
    document.querySelectorAll("[data-svg]").forEach(function (box) {
      if (!window.fetch) return;
      fetch(box.dataset.svg).then(function (r) { return r.ok ? r.text() : Promise.reject(); }).then(function (text) {
        box.innerHTML = text;
        var svg = box.querySelector("svg");
        if (!svg || !("IntersectionObserver" in window) || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
        svg.classList.add("th-anim");
        var seen = new IntersectionObserver(function (entries) {
          if (!entries[0].isIntersecting) return;
          seen.disconnect();
          setTimeout(function () { svg.classList.add("in"); }, 250);
        }, { threshold: 0.35 });
        seen.observe(svg);
      }, function () {});
    });

    // Agents marquee: the badges are duplicated once so the strip loops seamlessly; the speed is
    // set from its width so a longer list does not scroll faster. Reduced motion keeps a static row.
    var track = document.querySelector(".agents-track");
    if (track && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      Array.prototype.slice.call(track.children).forEach(function (el) {
        var c = el.cloneNode(true);
        c.setAttribute("aria-hidden", "true");
        track.appendChild(c);
      });
      track.parentNode.classList.add("on");
      track.style.animationDuration = Math.max(20, track.scrollWidth / 2 / 40) + "s";
    }
  });
})();
