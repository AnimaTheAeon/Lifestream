// Shared by the toolbar panel, the settings page and the background worker.

export const DEFAULTS = {
  address: "http://192.168.1.127:9091",
  username: "",
  password: "",
  intercept: true, // magnet links you click go straight to Lifestream
  paused: false, // add downloads paused
  look: "ffvii", // ffvii | xbox | blades
  midgarAddress: "http://192.168.1.127:8990",
  midgarKey: "",
  route: "lifestream", // lifestream | auto (TV shows to Midgar, the rest to Lifestream)
};

export const ROUTES = ["lifestream", "auto"];

export const LOOKS = ["ffvii", "xbox", "blades"];

export async function getSettings() {
  const s = await chrome.storage.local.get(Object.keys(DEFAULTS));
  const out = { ...DEFAULTS, ...s };
  if (!LOOKS.includes(out.look)) out.look = DEFAULTS.look;
  if (!ROUTES.includes(out.route)) out.route = DEFAULTS.route;
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

/** "192.168.1.127:8990/" -> "http://192.168.1.127:8990". Empty on nonsense. */
export function normalizeMidgar(text) {
  let t = String(text || "").trim();
  if (!t) return "";
  if (!/^https?:\/\//i.test(t)) t = "http://" + t;
  try {
    const u = new URL(t);
    return u.origin + u.pathname.replace(/\/+$/, "").replace(/\/api(\/v3)?$/i, "");
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
  held: "Midgar is holding it",
  refused: "Midgar didn't take it",
};

// ---------------------------------------------------------------- Midgar

/** Does the name look like a TV episode or season (S08, S08E03, 8x03, Season 8)? */
export function looksLikeTv(name) {
  return /(^|[^a-z0-9])s\d{1,2}(e\d{1,3})?([^a-z0-9]|$)/i.test(name) ||
    /(^|[^a-z0-9])\d{1,2}x\d{2,3}([^a-z0-9]|$)/i.test(name) ||
    /(^|[^a-z0-9])season[ ._-]?\d{1,2}([^a-z0-9]|$)/i.test(name) ||
    /complete[ ._-]series/i.test(name);
}

/** The release name Midgar should read: the magnet's name, or a .torrent link's file name. */
export function releaseTitle(link) {
  if (/^magnet:/i.test(link)) {
    const n = nameFromLink(link);
    return /^Magnet [0-9a-f]+…$/.test(n) || n === "Magnet link" ? "" : n;
  }
  return nameFromLink(link).replace(/\.torrent$/i, "");
}

async function midgarFetch(settings, path, init = {}, timeoutMs = 8000) {
  const address = normalizeMidgar(settings.midgarAddress);
  if (!address || !settings.midgarKey) throw new LsError("setup", "Set Midgar's address and API key in the settings first.");
  let r;
  try {
    r = await fetch(address + "/api/v3/" + path, {
      ...init,
      headers: { "X-Api-Key": settings.midgarKey.trim(), "Content-Type": "application/json", ...(init.headers || {}) },
      cache: "no-store",
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (e) {
    if (e && e.name === "TimeoutError") throw new LsError("timeout", "Midgar took too long to answer.");
    throw new LsError("offline", `Midgar wasn't reachable at ${hostOf(address)}. Is it running?`);
  }
  if (r.status === 401) throw new LsError("auth", "Midgar turned down the API key. Copy it again from Midgar's Settings › General.");
  return r;
}

/** Checks the address and key; returns Midgar's version. */
export async function midgarStatus(settings) {
  const r = await midgarFetch(settings, "system/status");
  if (!r.ok) throw new LsError("http", `Midgar answered with an error (${r.status}).`);
  const j = await r.json().catch(() => null);
  if (!j || !j.version) throw new LsError("http", "That address answered, but it doesn't look like Midgar.");
  return j;
}

/**
 * Hands a link to Midgar, which decides whether it wants it and, if so,
 * sends it to Lifestream itself. Returns
 *   { state: "sent" | "held" | "refused", name, reason, detail }
 * and throws LsError when Midgar can't be reached.
 */
export async function midgarPush(settings, link) {
  const title = releaseTitle(link);
  if (!title) {
    return { state: "refused", reason: "This magnet link has no name, so Midgar can't tell which show it is.", detail: "" };
  }
  const magnet = /^magnet:/i.test(link);
  const body = { title, protocol: "torrent", publishDate: new Date().toISOString(), [magnet ? "magnetUrl" : "downloadUrl"]: link };
  const r = await midgarFetch(settings, "release/push", { method: "POST", body: JSON.stringify(body) }, 45000);
  const j = await r.json().catch(() => null);
  if (r.status === 400) {
    const msgs = Array.isArray(j) ? j.map((v) => v.errorMessage).filter(Boolean) : [];
    return { state: "refused", reason: explainRejections(msgs.length ? msgs : ["Unable to parse"]), detail: msgs.join(" · ") };
  }
  if (!r.ok) {
    const msg = (j && (j.message || j.error)) || `Midgar answered with an error (${r.status}).`;
    throw new LsError("http", String(msg));
  }
  const d = Array.isArray(j) ? j[0] : j;
  if (!d) throw new LsError("http", "Midgar didn't say what it did with the link.");
  const rejections = (d.rejections || []).map(String);
  const name = describe(d) || title;
  if (d.approved) return { state: "sent", name, reason: "", detail: "" };
  if (d.temporarilyRejected) return { state: "held", name, reason: explainRejections(rejections, true), detail: rejections.join(" · ") };
  return { state: "refused", name, reason: explainRejections(rejections), detail: rejections.join(" · ") };
}

/** "The Vampire Diaries · Season 8" or "… · S08E03". */
function describe(d) {
  if (!d.seriesTitle) return "";
  const s = String(d.seasonNumber).padStart(2, "0");
  if (d.fullSeason) return `${d.seriesTitle} · Season ${d.seasonNumber}`;
  const eps = d.episodeNumbers || [];
  if (eps.length) return `${d.seriesTitle} · S${s}E${String(eps[0]).padStart(2, "0")}${eps.length > 1 ? "–E" + String(eps[eps.length - 1]).padStart(2, "0") : ""}`;
  return d.seriesTitle;
}

// Midgar's reasons are written for the app; these say the same in plain words.
const REASONS = [
  [/^Unknown Series/i, "This show isn't in Midgar yet. Add it in Midgar first, then send the link again."],
  [/^Unable to parse/i, "Midgar couldn't tell which show or episode this is from the link's name."],
  [/^Series is not monitored/i, "This show is unmonitored in Midgar, so it isn't collecting it."],
  [/not monitored/i, "Midgar isn't set to collect these episodes (they're unmonitored)."],
  [/^Existing file/i, "You already have these episodes in the same or better quality."],
  [/^Release in queue/i, "Midgar is already downloading these episodes in the same or better quality."],
  [/grab event in history/i, "Midgar grabbed these episodes recently in the same or better quality."],
  [/same (torrent hash|release name) as a grabbed/i, "Midgar already grabbed and imported this exact release."],
  [/is not wanted in profile/i, "This quality isn't one your quality profile accepts."],
  [/larger than maximum|smaller than minimum/i, "It's outside the size limits set for this quality."],
  [/blocklisted/i, "This release is on Midgar's blocklist (it failed before)."],
  [/ignored terms|required terms|Custom Format.*below/i, "Your release profile rules don't allow it."],
  [/All episodes haven't aired/i, "It's a full-season pack, but not every episode of that season has aired yet."],
  [/Multi-season releases/i, "Midgar can't take packs that cover several seasons. Send one season at a time."],
  [/Wrong (series|season|episode)|wasn't requested/i, "The link's name doesn't match the show or episodes Midgar expected."],
  [/ambiguous numbering/i, "Midgar couldn't tell for sure which episodes these are."],
  [/Torrent is not enabled/i, "This show's delay profile doesn't allow torrents."],
  [/Raw (Bluray|DVD)/i, "It's a raw disc copy, which Midgar doesn't take."],
  [/Proper|Repack/i, "It's a proper or repack, and Midgar is set not to grab those."],
];

export function explainRejections(list, held = false) {
  if (held) return "Midgar is holding it for a while (your delay settings) and will send it on by itself if nothing better turns up.";
  const out = [];
  for (const raw of list) {
    const hit = REASONS.find(([re]) => re.test(raw));
    const text = hit ? hit[1] : raw;
    if (!out.includes(text)) out.push(text);
  }
  return out.slice(0, 2).join(" ") || "Midgar didn't say why.";
}
