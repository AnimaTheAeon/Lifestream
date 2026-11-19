import { getSettings, rpc, nameFromLink, normalizeAddress, normalizeApp, appPush, guessTarget, TARGETS } from "./lib.js";

const MAX_RECENT = 12;

chrome.runtime.onInstalled.addListener((details) => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "send", title: "Send to Lifestream", contexts: ["link"] });
    chrome.contextMenus.create({ id: "send-paused", title: "Send to Lifestream, paused", contexts: ["link"] });
    chrome.contextMenus.create({ id: "send-midgar", title: "Send to Midgar (TV shows)", contexts: ["link"] });
    chrome.contextMenus.create({ id: "send-turks", title: "Send to Turks (movies)", contexts: ["link"] });
  });
  if (details.reason === "install") chrome.runtime.openOptionsPage();
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (!info.linkUrl) return;
  const target = { "send-midgar": "midgar", "send-turks": "turks" }[info.menuItemId] || "lifestream";
  send(info.linkUrl, info.menuItemId === "send-paused", tab && tab.id, undefined, target);
});

chrome.runtime.onMessage.addListener((msg, sender, reply) => {
  const tabId = sender.tab ? sender.tab.id : undefined;
  if (msg.type === "send") {
    send(msg.url, msg.paused, tabId, msg.id, msg.target).then(reply);
    return true;
  }
  if (msg.type === "retry") {
    retry(msg.id, tabId).then(reply);
    return true;
  }
  if (msg.type === "open") {
    openApp(msg.target || "lifestream");
  }
  return false;
});

// The recent list is only ever written here, one change at a time.
let queue = Promise.resolve();
function changeRecent(fn) {
  queue = queue.then(async () => {
    const { recent = [] } = await chrome.storage.local.get("recent");
    const next = fn(recent).slice(0, MAX_RECENT);
    await chrome.storage.local.set({ recent: next });
  });
  return queue;
}

function upsert(entry) {
  return changeRecent((list) => [entry, ...list.filter((e) => e.id !== entry.id)]);
}

// target: "lifestream", "midgar", "turks", or undefined to follow the
// "Where links go" setting ("ask" shows a chooser in the page instead).
async function send(url, paused, tabId, id, target) {
  const s = await getSettings();
  if (paused === undefined || paused === null) paused = s.paused;
  const name = nameFromLink(url);
  if (!TARGETS.includes(target)) {
    if (s.route === "ask" && tabId !== undefined && tabId >= 0) {
      // Nothing is sent until a button in the notice is clicked.
      const entry = { id: id || crypto.randomUUID(), url, name, paused: !!paused, at: Date.now(), state: "choose", guess: guessTarget(name) };
      const shown = await toast(tabId, entry, s.look);
      if (shown) return entry;
      target = entry.guess; // pages that can't show the chooser: use the guess
    } else {
      target = s.route === "lifestream" ? "lifestream" : guessTarget(name);
    }
  }
  const entry = { id: id || crypto.randomUUID(), url, name, paused: !!paused, at: Date.now(), state: "sending", target };
  await upsert(entry);
  toast(tabId, entry, s.look);
  try {
    if (target === "midgar" || target === "turks") {
      const r = await appPush(target, s, url);
      entry.state = r.state;
      if (r.name) entry.name = r.name;
      entry.reason = r.reason;
      entry.detail = r.detail;
    } else {
      const a = await rpc(s, "torrent-add", { filename: url, paused: !!paused }, 45000);
      const t = a["torrent-duplicate"] || a["torrent-added"] || {};
      entry.state = a["torrent-duplicate"] ? "already" : "sent";
      if (t.name) entry.name = t.name;
    }
  } catch (e) {
    entry.state = "failed";
    entry.error = e.message;
  }
  entry.at = Date.now();
  await upsert(entry);
  toast(tabId, entry, s.look);
  return entry;
}

async function retry(id, tabId) {
  const { recent = [] } = await chrome.storage.local.get("recent");
  const old = recent.find((e) => e.id === id);
  if (!old) return null;
  return send(old.url, old.paused, tabId, old.id, old.target);
}

async function openApp(target) {
  const s = await getSettings();
  const address = target === "lifestream" ? normalizeAddress(s.address) : normalizeApp(s[target + "Address"]);
  if (address) chrome.tabs.create({ url: address + "/" });
  else chrome.runtime.openOptionsPage();
}

// Each notice update is numbered, so the page can ignore one that arrives late
// (a "Sending…" that lands after the result would otherwise stay up).
let toastCount = 0;

// The notice always shows in the tab's top page, even for links inside frames.
// Returns whether it could be shown.
async function toast(tabId, entry, look) {
  if (tabId === undefined || tabId === null || tabId < 0) return false;
  const msg = { type: "toast", entry: { ...entry }, look, rev: Date.now() * 1000 + (toastCount++ % 1000) };
  try {
    await chrome.tabs.sendMessage(tabId, msg, { frameId: 0 });
    return true;
  } catch {
    // Tabs opened before the extension was loaded don't have it yet.
    try {
      await chrome.scripting.executeScript({ target: { tabId, frameIds: [0] }, files: ["content.js"] });
      await chrome.tabs.sendMessage(tabId, msg, { frameId: 0 });
      return true;
    } catch {
      // Pages like edge:// and the add-ons store can't show it.
      return false;
    }
  }
}
