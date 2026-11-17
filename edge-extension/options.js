import { getSettings, rpc, normalizeAddress, normalizeMidgar, midgarStatus, LOOKS, ROUTES } from "./lib.js";

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
    midgarAddress: normalizeMidgar($("midgarAddress").value) || $("midgarAddress").value.trim(),
    midgarKey: $("midgarKey").value.trim(),
  };
}

async function start() {
  const s = await getSettings();
  $("address").value = s.address;
  $("username").value = s.username;
  $("password").value = s.password;
  $("intercept").checked = s.intercept;
  $("paused").checked = s.paused;
  $("midgarAddress").value = s.midgarAddress;
  $("midgarKey").value = s.midgarKey;
  for (const r of document.querySelectorAll('input[name="route"]')) {
    r.checked = r.value === s.route;
    r.addEventListener("change", () => {
      if (r.checked && ROUTES.includes(r.value)) save({ route: r.value });
    });
  }
  applyLook(s.look);

  let t;
  for (const id of ["address", "username", "password", "midgarAddress", "midgarKey"]) {
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
  $("midgarAddress").addEventListener("change", () => {
    const n = normalizeMidgar($("midgarAddress").value);
    if (n) $("midgarAddress").value = n;
    save(current());
  });
  $("testMidgar").addEventListener("click", async () => {
    const r = $("resultMidgar");
    const c = current();
    if (!normalizeMidgar(c.midgarAddress)) {
      r.className = "sub badtxt";
      r.textContent = "That address doesn't look right. Try http://192.168.1.127:8990";
      return;
    }
    $("midgarAddress").value = normalizeMidgar(c.midgarAddress);
    await save(c);
    $("testMidgar").disabled = true;
    r.className = "sub";
    r.textContent = "Checking…";
    try {
      const st = await midgarStatus(c);
      r.className = "sub good";
      r.textContent = `Connected · ${st.appName || "Midgar"} ${st.version}`;
    } catch (e) {
      r.className = "sub badtxt";
      r.textContent = e.message;
    } finally {
      $("testMidgar").disabled = false;
    }
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
