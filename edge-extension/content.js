// Runs on every page: sends clicked magnet links to Lifestream and shows the
// small notice in the corner.
(() => {
  if (window.__lifestreamEdge) return;
  window.__lifestreamEdge = true;

  const settings = { intercept: true, look: "ffvii" };
  try {
    chrome.storage.local.get(["intercept", "look"], (s) => Object.assign(settings, s));
    chrome.storage.onChanged.addListener((ch) => {
      for (const k of ["intercept", "look"]) if (ch[k]) settings[k] = ch[k].newValue;
    });
  } catch {}

  document.addEventListener(
    "click",
    (e) => {
      if (!settings.intercept || e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
      const a = e.composedPath().find((n) => n && n.tagName === "A" && n.href);
      if (!a || !/^magnet:/i.test(a.href)) return;
      e.preventDefault();
      e.stopPropagation();
      try {
        chrome.runtime.sendMessage({ type: "send", url: a.href });
      } catch {
        // The extension was reloaded; this old copy can't reach it. Let the page handle it.
        location.href = a.href;
      }
    },
    true
  );

  if (window.top !== window) return; // notices only in the top page

  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.type === "toast") show(msg.entry, msg.look || settings.look);
  });

  let fontsAdded = false;
  function addFonts() {
    if (fontsAdded) return;
    fontsAdded = true;
    const f = (family, file, weight) => {
      try {
        const face = new FontFace(family, `url(${chrome.runtime.getURL("fonts/" + file)})`, { weight });
        face.load().then((ff) => document.fonts.add(ff)).catch(() => {});
      } catch {}
    };
    f("LsNunito", "nunito-700.woff2", "700");
    f("LsNunito", "nunito-800.woff2", "800");
    f("LsOrbitron", "orbitron-800.ttf", "800");
  }

  const CSS = `
:host{all:initial}
.wrap{position:fixed;right:18px;bottom:18px;z-index:2147483647;display:flex;flex-direction:column;gap:10px;align-items:flex-end;pointer-events:none}
.t{pointer-events:auto;box-sizing:border-box;width:320px;display:flex;flex-direction:column;gap:4px;padding:12px 16px;font:700 14px/1.35 LsNunito,"Segoe UI",sans-serif;opacity:0;transform:translateY(8px);transition:opacity .2s,transform .2s}
.t.in{opacity:1;transform:none}
.hd{display:flex;align-items:center;gap:8px}
.dot{flex:none;width:10px;height:10px;border-radius:50%}
.title{font-weight:800;font-size:15px}
.nm{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.err{font-size:13px}
.acts{display:flex;gap:14px}
button{all:unset;cursor:pointer;font-weight:800;font-size:13px}
button:hover{text-decoration:underline}
button:focus-visible{outline:2px solid currentColor;outline-offset:2px}
.x{margin-left:auto;font-size:16px;line-height:1;opacity:.8}
.sending .dot{animation:p 1s ease-in-out infinite}
@keyframes p{50%{opacity:.3}}
@media (prefers-reduced-motion:reduce){.t{transition:none}.sending .dot{animation:none}}

.ffvii{border:2px solid #d6d8e4;border-radius:8px;background:linear-gradient(160deg,#2e56be 0%,#1a2c96 40%,#121e78 62%,#06083a 100%);box-shadow:0 0 0 1px #2c2e46,0 8px 20px rgba(0,0,0,.35);color:#f4f4f8;text-shadow:2px 2px 0 rgba(0,0,0,.85)}
.ffvii button{color:#70ccff}
.ffvii .err{color:#ffc4c4}

.xbox{border:2px solid rgba(160,255,80,.75);background:linear-gradient(135deg,rgba(26,86,12,.97) 0%,rgba(14,60,8,.97) 40%,rgba(3,22,3,.98) 100%);clip-path:polygon(14px 0,100% 0,100% calc(100% - 14px),calc(100% - 14px) 100%,0 100%,0 14px);color:#e4ffc4}
.xbox .title{font:800 12px/1.3 LsOrbitron,"Segoe UI",sans-serif;letter-spacing:.12em;text-transform:uppercase;color:#dcff64}
.xbox button{font:800 11px/1.3 LsOrbitron,"Segoe UI",sans-serif;letter-spacing:.12em;text-transform:uppercase;color:#c8ff4a}
.xbox .err{color:#ffb49c}

.blades{padding:12px 18px;border:1px solid rgba(255,255,255,.55);border-radius:18px;background:linear-gradient(160deg,#4a9a32 0%,#357a22 45%,#2a6619 100%);box-shadow:inset 0 1px 0 rgba(255,255,255,.5),0 8px 20px rgba(0,0,0,.3);color:#fff;text-shadow:0 1px 2px rgba(0,0,0,.35)}
.blades button{color:#fff}
.blades .err{color:#ffe0dc}

.st-sent .dot{background:#7cf09c;box-shadow:0 0 6px #7cf09c}
.xbox.st-sent .dot{background:#8cff6e}
.st-already .dot{background:#ffd06a}
.xbox.st-already .dot{background:#ffe14a}
.st-failed .dot{background:#ff8a8a;box-shadow:0 0 6px #ff8a8a}
.blades.st-failed .dot{background:#ffb0a0}
.st-sending .dot{background:#d6d8e4}
`;

  let host, wrap;
  const shown = new Map(); // entry id -> {el, timer}

  function ensureHost() {
    if (host && host.isConnected) return;
    host = document.createElement("lifestream-notice");
    const root = host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = CSS;
    wrap = document.createElement("div");
    wrap.className = "wrap";
    wrap.setAttribute("role", "status");
    wrap.setAttribute("aria-live", "polite");
    root.append(style, wrap);
    (document.body || document.documentElement).appendChild(host);
    shown.clear();
  }

  const TITLES = {
    sending: "Sending to Lifestream…",
    sent: "Sent to Lifestream",
    already: "Already in Lifestream",
    failed: "Didn't send",
  };

  function show(entry, look) {
    addFonts();
    ensureHost();
    let item = shown.get(entry.id);
    if (!item) {
      const el = document.createElement("div");
      item = { el, timer: 0, hover: false };
      el.addEventListener("mouseenter", () => {
        item.hover = true;
        clearTimeout(item.timer);
      });
      el.addEventListener("mouseleave", () => {
        item.hover = false;
        schedule(entry.id, item, 2500);
      });
      wrap.appendChild(el);
      shown.set(entry.id, item);
      requestAnimationFrame(() => el.classList.add("in"));
    }
    const el = item.el;
    el.className = `t ${look} st-${entry.state}${entry.state === "sending" ? " sending" : ""}${el.classList.contains("in") ? " in" : ""}`;
    el.textContent = "";

    const hd = document.createElement("div");
    hd.className = "hd";
    const dot = document.createElement("span");
    dot.className = "dot";
    const title = document.createElement("span");
    title.className = "title";
    title.textContent = TITLES[entry.state] || "";
    const x = document.createElement("button");
    x.className = "x";
    x.textContent = "×";
    x.setAttribute("aria-label", "Close");
    x.addEventListener("click", () => hide(entry.id));
    hd.append(dot, title, x);

    const nm = document.createElement("div");
    nm.className = "nm";
    nm.textContent = entry.name;
    nm.title = entry.name;
    el.append(hd, nm);

    if (entry.state === "failed" && entry.error) {
      const err = document.createElement("div");
      err.className = "err";
      err.textContent = entry.error;
      el.append(err);
    }
    if (entry.state !== "sending") {
      const acts = document.createElement("div");
      acts.className = "acts";
      if (entry.state === "failed") {
        const r = document.createElement("button");
        r.textContent = "Retry";
        r.addEventListener("click", () => chrome.runtime.sendMessage({ type: "retry", id: entry.id }));
        acts.append(r);
      }
      const o = document.createElement("button");
      o.textContent = "Open Lifestream";
      o.addEventListener("click", () => chrome.runtime.sendMessage({ type: "open" }));
      acts.append(o);
      el.append(acts);
    }
    clearTimeout(item.timer);
    if (entry.state !== "sending" && !item.hover) schedule(entry.id, item, entry.state === "failed" ? 12000 : 5000);
  }

  function schedule(id, item, ms) {
    clearTimeout(item.timer);
    item.timer = setTimeout(() => hide(id), ms);
  }

  function hide(id) {
    const item = shown.get(id);
    if (!item) return;
    shown.delete(id);
    clearTimeout(item.timer);
    item.el.classList.remove("in");
    setTimeout(() => item.el.remove(), 250);
  }
})();
