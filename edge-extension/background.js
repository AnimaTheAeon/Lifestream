import { getSettings, rpc, nameFromLink, normalizeAddress } from "./lib.js";

const MAX_RECENT = 12;

chrome.runtime.onInstalled.addListener((details) => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "send", title: "Send to Lifestream", contexts: ["link"] });
    chrome.contextMenus.create({ id: "send-paused", title: "Send to Lifestream, paused", contexts: ["link"] });
  });
  if (details.reason === "install") chrome.runtime.openOptionsPage();
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (!info.linkUrl) return;
  send(info.linkUrl, info.menuItemId === "send-paused", tab && tab.id);
});

chrome.runtime.onMessage.addListener((msg, sender, reply) => {
  const tabId = sender.tab ? sender.tab.id : undefined;
  if (msg.type === "send") {
    send(msg.url, msg.paused, tabId).then(reply);
    return true;
  }
  if (msg.type === "retry") {
    retry(msg.id, tabId).then(reply);
    return true;
  }
  if (msg.type === "open") {
    openLifestream();
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

async function send(url, paused, tabId, id) {
  const s = await getSettings();
  if (paused === undefined || paused === null) paused = s.paused;
  const entry = { id: id || crypto.randomUUID(), url, name: nameFromLink(url), paused: !!paused, at: Date.now(), state: "sending" };
  await upsert(entry);
  toast(tabId, entry, s.look);
  try {
    const a = await rpc(s, "torrent-add", { filename: url, paused: !!paused }, 45000);
    const t = a["torrent-duplicate"] || a["torrent-added"] || {};
    entry.state = a["torrent-duplicate"] ? "already" : "sent";
    if (t.name) entry.name = t.name;
    delete entry.error;
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
  return send(old.url, old.paused, tabId, old.id);
}

async function openLifestream() {
  const s = await getSettings();
  const address = normalizeAddress(s.address);
  if (address) chrome.tabs.create({ url: address + "/" });
  else chrome.runtime.openOptionsPage();
}

// The notice always shows in the tab's top page, even for links inside frames.
async function toast(tabId, entry, look) {
  if (tabId === undefined || tabId === null || tabId < 0) return;
  const msg = { type: "toast", entry, look };
  try {
    await chrome.tabs.sendMessage(tabId, msg, { frameId: 0 });
  } catch {
    // Tabs opened before the extension was loaded don't have it yet.
    try {
      await chrome.scripting.executeScript({ target: { tabId, frameIds: [0] }, files: ["content.js"] });
      await chrome.tabs.sendMessage(tabId, msg, { frameId: 0 });
    } catch {
      // Pages like edge:// and the add-ons store can't show it. That's fine.
    }
  }
}
