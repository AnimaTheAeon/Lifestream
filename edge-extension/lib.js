// Shared by the toolbar panel, the settings page and the background worker.

export const DEFAULTS = {
  address: "http://192.168.1.127:9091",
  username: "",
  password: "",
  intercept: true, // magnet links you click go straight to Lifestream
  paused: false, // add downloads paused
  look: "ffvii", // ffvii | xbox | blades
};

export const LOOKS = ["ffvii", "xbox", "blades"];

export async function getSettings() {
  const s = await chrome.storage.local.get(Object.keys(DEFAULTS));
  const out = { ...DEFAULTS, ...s };
  if (!LOOKS.includes(out.look)) out.look = DEFAULTS.look;
  return out;
}

/** "192.168.1.127:9091/" -> "http://192.168.1.127:9091". Empty on nonsense. */
export function normalizeAddress(text) {
  let t = String(text || "").trim();
  if (!t) return "";
  if (!/^https?:\/\//i.test(t)) t = "http://" + t;
  try {
    const u = new URL(t);
    let path = u.pathname.replace(/\/+$/, "").replace(/\/transmission(\/(rpc|web))?$/i, "");
    return u.origin + path;
  } catch {
    return "";
  }
}

export function hostOf(address) {
  try {
    return new URL(address).host;
  } catch {
    return address;
  }
}

export class LsError extends Error {
  constructor(kind, message) {
    super(message);
    this.kind = kind;
  }
}

let sessionId = "";

function basic(user, pass) {
  const bytes = new TextEncoder().encode(`${user}:${pass}`);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return "Basic " + btoa(bin);
}

/** One Transmission-style RPC call to Lifestream. Throws LsError with a plain message. */
export async function rpc(settings, method, args = {}, timeoutMs = 8000) {
  const address = normalizeAddress(settings.address);
  if (!address) throw new LsError("setup", "Set Lifestream's address in the settings first.");
  const url = address + "/transmission/rpc";
  const headers = { "Content-Type": "application/json", "X-Lifestream": "edge" };
  if (settings.username || settings.password) headers.Authorization = basic(settings.username, settings.password);
  const body = JSON.stringify({ method, arguments: args });
  for (let attempt = 0; attempt < 3; attempt++) {
    if (sessionId) headers["X-Transmission-Session-Id"] = sessionId;
    let r;
    try {
      r = await fetch(url, { method: "POST", headers, body, credentials: "include", cache: "no-store", signal: AbortSignal.timeout(timeoutMs) });
    } catch (e) {
      if (e && e.name === "TimeoutError") throw new LsError("timeout", "Lifestream took too long to answer.");
      throw new LsError("offline", `Lifestream wasn't reachable at ${hostOf(address)}. Is it running, and is Barrier up?`);
    }
    if (r.status === 409) {
      sessionId = r.headers.get("X-Transmission-Session-Id") || "";
      continue;
    }
    if (r.status === 401) throw new LsError("auth", "Lifestream turned down the username or password.");
    if (r.status === 403) {
      const text = (await r.text().catch(() => "")).trim();
      throw new LsError("denied", text || "Lifestream doesn't allow other devices yet.");
    }
    if (!r.ok) throw new LsError("http", `Lifestream answered with an error (${r.status}).`);
    let j;
    try {
      j = await r.json();
    } catch {
      throw new LsError("http", "That address answered, but it doesn't look like Lifestream.");
    }
    if (j.result !== "success") throw new LsError("rpc", cap(j.result || "Lifestream couldn't do that."));
    return j.arguments || {};
  }
  throw new LsError("http", "Lifestream kept asking for a new session. Try again.");
}

function cap(s) {
  s = String(s);
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** A readable name for a magnet or .torrent link. */
export function nameFromLink(link) {
  if (/^magnet:/i.test(link)) {
    const q = link.slice(link.indexOf("?") + 1);
    for (const part of q.split("&")) {
      const [k, v = ""] = part.split("=");
      if (k.toLowerCase() === "dn" && v) {
        try {
          return decodeURIComponent(v.replace(/\+/g, " "));
        } catch {
          return v;
        }
      }
    }
    const m = /xt=urn:btih:([0-9a-z]+)/i.exec(link);
    return m ? `Magnet ${m[1].slice(0, 12).toLowerCase()}…` : "Magnet link";
  }
  try {
    const u = new URL(link);
    const last = decodeURIComponent(u.pathname.split("/").filter(Boolean).pop() || "");
    return last || u.host;
  } catch {
    return link;
  }
}

export function isSendable(link) {
  return /^magnet:\?/i.test(link) || /^https?:\/\/\S+$/i.test(link);
}

export function rate(bytesPerSec) {
  const b = Number(bytesPerSec) || 0;
  if (b < 1024) return `${b} B/s`;
  if (b < 1024 * 1024) return `${Math.round(b / 1024)} KB/s`;
  return `${(b / 1024 / 1024).toFixed(1)} MB/s`;
}

export function ago(ms) {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 45) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  return `${Math.round(h / 24)} d ago`;
}

export const STATE_TEXT = {
  sending: "Sending…",
  sent: "Sent",
  already: "Already in Lifestream",
  failed: "Didn't send",
};
