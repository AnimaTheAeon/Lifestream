import { getSettings, rpc, hostOf, normalizeAddress, rate, ago, isSendable, STATE_TEXT } from "./lib.js";

const $ = (id) => document.getElementById(id);
let settings;

async function start() {
  settings = await getSettings();
  document.body.className = "look-" + settings.look;
  $("paused").checked = settings.paused;
  $("send").textContent = "Send";
  $("stAddr").textContent = hostOf(normalizeAddress(settings.address) || settings.address);
  renderRecent((await chrome.storage.local.get("recent")).recent || []);
  refresh();
  setInterval(refresh, 2000);
  setInterval(async () => renderRecent((await chrome.storage.local.get("recent")).recent || []), 30000);
  $("mag").focus();
}

function setStatus(kind, title, nums) {
  for (const el of [$("status"), $("pill")]) {
    el.classList.remove("ok", "bad", "wait");
    el.classList.add(kind);
  }
  $("stTitle").textContent = title;
  $("pillText").textContent = kind === "ok" ? "All good" : kind === "bad" ? "Can't connect" : "Checking…";
  const n = $("nums");
  n.textContent = "";
  if (typeof nums === "string") {
    n.textContent = nums;
    return;
  }
  if (!nums) return;
  const host = hostOf(normalizeAddress(settings.address));
  if (settings.look === "ffvii") {
    for (const [k, v] of [["ACTIVE", nums.active], ["DOWN", nums.down], ["UP", nums.up]]) {
      const s = document.createElement("span");
      const b = document.createElement("span");
      b.className = "k";
      b.textContent = k;
      s.append(b, " " + v);
      n.append(s);
    }
  } else if (settings.look === "xbox") {
    n.textContent = `${nums.active} active · ↓ ${nums.down} · ↑ ${nums.up}`;
  } else {
    n.textContent = `${host} · ${nums.active} active · ↓ ${nums.down} · ↑ ${nums.up}`;
  }
}

let busy = false;
async function refresh() {
  if (busy) return;
  busy = true;
  try {
    const st = await rpc(settings, "session-stats", {}, 5000);
    setStatus("ok", "Connected", { active: st.activeTorrentCount ?? 0, down: rate(st.downloadSpeed), up: rate(st.uploadSpeed) });
  } catch (e) {
    setStatus("bad", "Can't connect", e.message);
  } finally {
    busy = false;
  }
}

function renderRecent(list) {
  const ul = $("recent");
  ul.textContent = "";
  if (!list.length) {
    const li = document.createElement("li");
    li.className = "empty";
    li.textContent = "Nothing sent yet. Click a magnet link on any page, or paste one above.";
    ul.append(li);
    return;
  }
  for (const e of list) {
    const li = document.createElement("li");
    li.className = "item st-" + e.state;
    const dot = document.createElement("span");
    dot.className = "dot";
    const tx = document.createElement("div");
    tx.className = "tx";
    const nm = document.createElement("div");
    nm.className = "nm";
    nm.textContent = e.name;
    nm.title = e.name;
    const sub = document.createElement("div");
    sub.className = "sub";
    let text = STATE_TEXT[e.state] || e.state;
    if (e.target === "midgar" && (e.state === "sent" || e.state === "sending")) text = e.state === "sent" ? "Sent to Midgar" : "Sending to Midgar…";
    if (e.target === "midgar" && e.state === "failed") text = "Couldn't reach Midgar";
    if (e.state === "failed" && e.error) text += ": " + e.error;
    if (e.state === "refused" && e.reason) text += ": " + e.reason;
    if (e.state === "held" && e.reason) text = e.reason;
    if (e.paused && e.state !== "failed") text += " (paused)";
    if (e.state !== "sending") text += " · " + ago(e.at);
    sub.append(text);
    if (e.state === "refused") {
      const l = document.createElement("button");
      l.className = "retry";
      l.textContent = "Send to Lifestream";
      l.addEventListener("click", () => chrome.runtime.sendMessage({ type: "send", url: e.url, target: "lifestream" }));
      sub.append(" · ", l);
    }
    if (e.state === "failed") {
      const r = document.createElement("button");
      r.className = "retry";
      r.textContent = "Retry";
      r.addEventListener("click", () => chrome.runtime.sendMessage({ type: "retry", id: e.id }));
      sub.append(" · ", r);
    }
    tx.append(nm, sub);
    li.append(dot, tx);
    ul.append(li);
  }
}

chrome.storage.onChanged.addListener((ch) => {
  if (ch.recent) renderRecent(ch.recent.newValue || []);
});

$("form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const url = $("mag").value.trim();
  const err = $("formErr");
  if (!isSendable(url)) {
    err.textContent = url ? "That doesn't look like a magnet link." : "Paste a magnet link first.";
    err.classList.remove("hidden");
    return;
  }
  err.classList.add("hidden");
  $("mag").value = "";
  $("send").disabled = true;
  try {
    await chrome.runtime.sendMessage({ type: "send", url, paused: $("paused").checked });
  } finally {
    $("send").disabled = false;
    $("mag").focus();
  }
});

$("mag").addEventListener("input", () => $("formErr").classList.add("hidden"));
$("settings").addEventListener("click", () => {
  chrome.runtime.openOptionsPage();
  window.close();
});
$("open").addEventListener("click", (e) => {
  e.preventDefault();
  chrome.runtime.sendMessage({ type: "open" });
  window.close();
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") window.close();
});

start();
