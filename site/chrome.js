/* Short entrance lives in CSS. This file is current-page chrome,
   a quiet click, and a mute control. Sounds are synthesized sines —
   the same family as the editor cues — and will not stack. */

(function () {
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var heroVideo = document.querySelector(".hero-video");
  if (reduce && heroVideo) {
    heroVideo.pause();
    heroVideo.removeAttribute("autoplay");
    document.documentElement.classList.add("reduce-motion");
  }

  var path = normalize(location.pathname);
  document.querySelectorAll(".nav-links a").forEach(function (link) {
    var href = link.getAttribute("href");
    if (!href || href.charAt(0) === "#") return;
    if (/^https?:\/\//.test(href)) return;
    var target = normalize(href);
    if (target === path) link.setAttribute("aria-current", "page");
  });

  var stored = null;
  try {
    stored = localStorage.getItem("kerabit-sound");
  } catch (err) {
    stored = null;
  }
  var enabled = stored === "on" ? true : stored === "off" ? false : !reduce;
  var audioCtx = null;
  var lastHover = 0;
  var lastPress = 0;

  var toggle = mountToggle();
  paintToggle(toggle);

  document.addEventListener("pointerover", function (event) {
    if (event.pointerType === "touch") return;
    var el = actionable(event.target);
    if (!el) return;
    if (event.relatedTarget && el.contains(event.relatedTarget)) return;
    cue("hover");
  });

  document.addEventListener("pointerdown", function (event) {
    var el = actionable(event.target);
    if (!el) return;
    if (el === toggle) {
      setSound(!enabled);
      return;
    }
    cue("press");
  });

  document.addEventListener("keydown", function (event) {
    if (event.repeat) return;
    var el = actionable(event.target);
    if (!el || el !== document.activeElement) return;
    if (el === toggle && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      setSound(!enabled);
      return;
    }
    if (event.key === "Enter") cue("press");
  });

  function normalize(value) {
    return String(value || "")
      .replace(/\/index\.html$/, "")
      .replace(/\.html$/, "")
      .replace(/\/$/, "") || "/";
  }

  function actionable(node) {
    if (!node || !node.closest) return null;
    return node.closest(
      ".nav-brand, .nav-links a, .btn, .docs-cards a, .foot a, .sound-toggle"
    );
  }

  function mountToggle() {
    var foot = document.querySelector(".foot");
    if (!foot) return null;
    var existing = foot.querySelector(".sound-toggle");
    if (existing) return existing;
    var actions = document.createElement("div");
    actions.className = "foot-actions";
    var button = document.createElement("button");
    button.type = "button";
    button.className = "sound-toggle";
    var link = foot.querySelector("a");
    actions.appendChild(button);
    if (link) actions.appendChild(link);
    foot.appendChild(actions);
    return button;
  }

  function paintToggle(button) {
    if (!button) return;
    button.setAttribute("aria-pressed", enabled ? "true" : "false");
    button.textContent = enabled ? "Sound on" : "Sound off";
  }

  function setSound(next) {
    if (!next) cue("press");
    enabled = next;
    persist();
    paintToggle(toggle);
    if (next) cue("press");
  }

  function persist() {
    try {
      localStorage.setItem("kerabit-sound", enabled ? "on" : "off");
    } catch (err) {
      /* private mode: the toggle still works for this view */
    }
  }

  function cue(kind) {
    if (!enabled) return;
    var now = performance.now();
    if (kind === "press") {
      if (now - lastPress < 45) return;
      lastPress = now;
    } else {
      if (now - lastHover < 110) return;
      lastHover = now;
    }
    try {
      blip(kind);
    } catch (err) {
      /* a missed tick should not block the click */
    }
  }

  function blip(kind) {
    var ctx = context();
    if (!ctx) return;
    if (ctx.state === "suspended") {
      ctx.resume();
    }
    var now = ctx.currentTime;
    var osc = ctx.createOscillator();
    var gain = ctx.createGain();
    osc.type = "sine";
    var dur = kind === "press" ? 0.065 : 0.045;
    if (kind === "press") {
      osc.frequency.setValueAtTime(698, now);
      osc.frequency.exponentialRampToValueAtTime(988, now + 0.048);
      gain.gain.setValueAtTime(0.0001, now);
      gain.gain.exponentialRampToValueAtTime(0.04, now + 0.008);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.06);
    } else {
      osc.frequency.setValueAtTime(1318, now);
      osc.frequency.exponentialRampToValueAtTime(1568, now + 0.036);
      gain.gain.setValueAtTime(0.0001, now);
      gain.gain.exponentialRampToValueAtTime(0.018, now + 0.006);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.04);
    }
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(now);
    osc.stop(now + dur);
  }

  function context() {
    var AC = window.AudioContext || window.webkitAudioContext;
    if (!AC) return null;
    if (!audioCtx) audioCtx = new AC();
    return audioCtx;
  }
})();
