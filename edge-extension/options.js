import { getSettings, rpc, normalizeAddress, LOOKS } from "./lib.js";

const $ = (id) => document.getElementById(id);

function applyLook(look) {
  document.body.className = "look-" + look;
  for (const b of document.querySelectorAll(".seg button")) b.setAttribute("aria-pressed", String(b.dataset.look === look));
}

let savedTimer;
async function save(patch) {
  await chrome.storage.local.set(patch);
  $("saved").textContent = "Saved";
  clearTimeout(savedTimer);
  savedTimer = setTimeout(() => ($("saved").textContent = ""), 1500);
}

function current() {
  return {
    address: normalizeAddress($("address").value) || $("address").value.trim(),
    username: $("username").value,
    password: $("password").value,
  };
}

async function start() {
  const s = await getSettings();
  $("address").value = s.address;
  $("username").value = s.username;
  $("password").value = s.password;
  $("intercept").checked = s.intercept;
  $("paused").checked = s.paused;
  applyLook(s.look);

  let t;
  for (const id of ["address", "username", "password"]) {
    $(id).addEventListener("input", () => {
      clearTimeout(t);
      t = setTimeout(() => save(current()), 400);
    });
  }
  $("address").addEventListener("change", () => {
    const n = normalizeAddress($("address").value);
    if (n) $("address").value = n;
    save(current());
  });
  $("intercept").addEventListener("change", () => save({ intercept: $("intercept").checked }));
  $("paused").addEventListener("change", () => save({ paused: $("paused").checked }));
  for (const b of document.querySelectorAll(".seg button")) {
    b.addEventListener("click", () => {
      if (!LOOKS.includes(b.dataset.look)) return;
      applyLook(b.dataset.look);
      save({ look: b.dataset.look });
    });
  }

  $("test").addEventListener("click", async () => {
    const r = $("result");
    const c = current();
    if (!normalizeAddress(c.address)) {
      r.className = "sub badtxt";
      r.textContent = "That address doesn't look right. Try http://192.168.1.127:9091";
      return;
    }
    $("address").value = normalizeAddress(c.address);
    await save(c);
    $("test").disabled = true;
    r.className = "sub";
    r.textContent = "Checking…";
    try {
      const a = await rpc(c, "session-get", {}, 8000);
      const m = /Lifestream ([\w.\-]+)/.exec(a.version || "");
      r.className = "sub good";
      r.textContent = m ? `Connected · Lifestream ${m[1]}` : "Connected, but this looks like a different torrent app.";
    } catch (e) {
      r.className = "sub badtxt";
      r.textContent = e.message;
    } finally {
      $("test").disabled = false;
    }
  });
}

start();
