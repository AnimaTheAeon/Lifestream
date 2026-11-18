import { getSettings, rpc, normalizeAddress, normalizeApp, appStatus, LOOKS, ROUTES } from "./lib.js";

const $ = (id) => document.getElementById(id);

function applyLook(look) {
  document.body.className = "look-" + look;
  for (const b of document.querySelectorAll(".seg button")) b.setAttribute("aria-pressed", String(b.dataset.look === look));
  placePill();
}

// Dark Glass: the selection pill glides under the picked look.
function placePill() {
  const seg = document.querySelector(".seg");
  const on = seg && seg.querySelector('button[aria-pressed="true"]');
  if (!on) return;
  seg.style.setProperty("--sel-left", on.offsetLeft + "px");
  seg.style.setProperty("--sel-w", on.offsetWidth + "px");
}
addEventListener("resize", placePill);
document.fonts && document.fonts.ready.then(placePill);

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
    midgarAddress: normalizeApp($("midgarAddress").value) || $("midgarAddress").value.trim(),
    midgarKey: $("midgarKey").value.trim(),
    turksAddress: normalizeApp($("turksAddress").value) || $("turksAddress").value.trim(),
    turksKey: $("turksKey").value.trim(),
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
  $("turksAddress").value = s.turksAddress;
  $("turksKey").value = s.turksKey;
  for (const r of document.querySelectorAll('input[name="route"]')) {
    r.checked = r.value === s.route;
    r.addEventListener("change", () => {
      if (r.checked && ROUTES.includes(r.value)) save({ route: r.value });
    });
  }
  applyLook(s.look);

  let t;
  for (const id of ["address", "username", "password", "midgarAddress", "midgarKey", "turksAddress", "turksKey"]) {
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
  for (const [app, name, example] of [["midgar", "Midgar", "http://192.168.1.127:8990"], ["turks", "Turks", "http://192.168.1.127:7878"]]) {
    const addr = $(app + "Address");
    const cap = app[0].toUpperCase() + app.slice(1);
    addr.addEventListener("change", () => {
      const n = normalizeApp(addr.value);
      if (n) addr.value = n;
      save(current());
    });
    $("test" + cap).addEventListener("click", async () => {
      const r = $("result" + cap);
      const c = current();
      if (!normalizeApp(c[app + "Address"])) {
        r.className = "sub badtxt";
        r.textContent = `That address doesn't look right. Try ${example}`;
        return;
      }
      addr.value = normalizeApp(c[app + "Address"]);
      await save(c);
      $("test" + cap).disabled = true;
      r.className = "sub";
      r.textContent = "Checking…";
      try {
        const st = await appStatus(app, c);
        const version = String(st.version).replace(/^5\./, "");
        r.className = "sub good";
        r.textContent = `Connected · ${name} ${version}`;
      } catch (e) {
        r.className = "sub badtxt";
        r.textContent = e.message;
      } finally {
        $("test" + cap).disabled = false;
      }
    });
  }
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
