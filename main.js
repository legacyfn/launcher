"use strict";
const { app, BrowserWindow, ipcMain, dialog, shell, safeStorage } = require("electron");
const path = require("path");
const fs = require("fs");
const https = require("https");
const { spawn, execFile } = require("child_process");
const { Worker } = require("worker_threads");

const { download } = require("./lib/downloader");
const { findBuildRoot, GAME_EXE_REL } = require("./lib/buildroot");
const { DEFAULT_ARGS, buildLaunchArgs } = require("./lib/args");

/* ------------------------------------------------------------------ */
/* Constants                                                           */
/* ------------------------------------------------------------------ */

const OFFICIAL_SITE = "https://projectlegacy.pages.dev/";
function loadEndpoints() {
  const d = {
    xmppServerUrl: "https://pr-xmpp.xeros-radio.nl",
    buildArchiveUrl: "https://projectlegacybuilds.streamxerosradio.duckdns.org/builds/4.5.rar",
    newsUrl: "https://projectlegacy.pages.dev/news.json",
    healthPollSeconds: 20
  };
  try { Object.assign(d, JSON.parse(fs.readFileSync(path.join(__dirname, "endpoints.json"), "utf8"))); } catch {}
  return d;
}
const ENDPOINTS = loadEndpoints();
const BUILD_URL = ENDPOINTS.buildArchiveUrl;
const HEALTH_URL = ENDPOINTS.xmppServerUrl + "/health";
const NEWS_URL = ENDPOINTS.newsUrl;
const DISCORD_URL = "https://discord.gg/fx4Gu3MAK";
const EVENTS_URL = ENDPOINTS.xmppServerUrl + "/events";
const CLIENTS_URL = ENDPOINTS.xmppServerUrl + "/clients";
const USER_AGENT = "ProjectLegacyLauncher/3.0";

const ARCHIVE_NAME = "4.5.rar";
const WORK_DIR_NAME = ".launcher";

// Files shipped with the launcher (see /bin).
const BIN_DIR = path.join(__dirname, "bin");
const INJECTOR = path.join(BIN_DIR, "Injector.exe");

// DLLs loaded into the game, in order. `delayMs` is counted from the moment the game
// process starts. If the game crashes at startup or the console does not unlock,
// these two numbers are the first thing to tune.
const INJECTIONS = [
  { label: "Redirector", file: "projectlegacy.dll", delayMs: 2500 }
];

// Same as the official launcher: the console and anti-crash modules are injected by hand
// from the Dashboard once the player is in the lobby (console is then opened with F3).
const MANUAL_MODULES = {
  console: { label: "Console", file: "edit.dll" },
  anticrash: { label: "Anti-crash", file: "anticrash.dll", optional: true }
};

// Placeholder processes the official launcher also starts (then freezes).
const STUB_EXES = [
  "FortniteLauncher.exe",
  "FortniteClient-Win64-Shipping_EAC.exe",
  "FortniteClient-Win64-Shipping_BE.exe"
];

/* ------------------------------------------------------------------ */
/* State                                                               */
/* ------------------------------------------------------------------ */

let mainWindow;
let eventRequest;

let config = {
  gameFolder: "",
  email: "",
  passwordEnc: "",
  passwordMode: "",
  extraArgs: DEFAULT_ARGS,
  useStubs: true,
  injectDelayMs: 2500,
  injectDelayVersion: 2,
  compat: false
};

let install = { phase: "idle" }; // idle | downloading | paused | extracting | done | error
let installBusy = false;
let downloadHandle = null;
let extractWorker = null;
let cancelRequested = false;

let autoRetries = 0;
let lastCrash = null; // { runtime, delay }
let game = { state: "idle", message: "", pid: null, stubs: [] }; // idle | launching | running

/* ------------------------------------------------------------------ */
/* Config                                                              */
/* ------------------------------------------------------------------ */

function configPath() {
  return path.join(app.getPath("userData"), "launcher-config.json");
}

function loadConfig() {
  try {
    const saved = JSON.parse(fs.readFileSync(configPath(), "utf8"));
    if (typeof saved.gameFolder === "string") config.gameFolder = saved.gameFolder;
    if (typeof saved.email === "string") config.email = saved.email;
    if (typeof saved.passwordEnc === "string") config.passwordEnc = saved.passwordEnc;
    if (typeof saved.passwordMode === "string") config.passwordMode = saved.passwordMode;
    if (typeof saved.extraArgs === "string") config.extraArgs = saved.extraArgs;
    // Earlier versions of this launcher saved different default arguments.
    if (config.extraArgs === "-epicapp=Fortnite -epicenv=Prod -epiclocale=en-us -epicportal -skippatchcheck -NOSSLPINNING") {
      config.extraArgs = DEFAULT_ARGS;
    }
    if (typeof saved.useStubs === "boolean") config.useStubs = saved.useStubs;
    if (typeof saved.compat === "boolean") config.compat = saved.compat;
    if (saved.injectDelayVersion === 2 && Number.isFinite(saved.injectDelayMs) && saved.injectDelayMs >= 500 && saved.injectDelayMs <= 30000) config.injectDelayMs = saved.injectDelayMs;
  } catch {
    /* first run */
  }
}

function saveConfig() {
  fs.mkdirSync(path.dirname(configPath()), { recursive: true });
  fs.writeFileSync(configPath(), JSON.stringify(config, null, 2));
}

function storePassword(password) {
  if (!password) {
    config.passwordEnc = "";
    config.passwordMode = "";
  } else if (safeStorage.isEncryptionAvailable()) {
    config.passwordEnc = safeStorage.encryptString(password).toString("base64");
    config.passwordMode = "safe";
  } else {
    config.passwordEnc = Buffer.from(password, "utf8").toString("base64");
    config.passwordMode = "plain";
  }
}

function readPassword() {
  if (!config.passwordEnc) return "";
  try {
    const raw = Buffer.from(config.passwordEnc, "base64");
    return config.passwordMode === "safe" ? safeStorage.decryptString(raw) : raw.toString("utf8");
  } catch {
    return "";
  }
}

// A build is only usable if the game files exist AND an interrupted extraction
// has not left the half-written marker behind.
function incompleteMarker() {
  return path.join(config.gameFolder || "", WORK_DIR_NAME, "incomplete");
}

function installedRoot() {
  const root = findBuildRoot(config.gameFolder);
  if (!root) return null;
  return fs.existsSync(incompleteMarker()) ? null : root;
}

function publicConfig() {
  const root = installedRoot();
  return {
    gameFolder: config.gameFolder,
    email: config.email,
    hasPassword: Boolean(config.passwordEnc),
    extraArgs: config.extraArgs,
    useStubs: config.useStubs,
    hasAnticrash: fs.existsSync(path.join(BIN_DIR, MANUAL_MODULES.anticrash.file)),
    installed: Boolean(root),
    buildRoot: root || "",
    platform: process.platform
  };
}

/* ------------------------------------------------------------------ */
/* Helpers                                                             */
/* ------------------------------------------------------------------ */

const sleep = ms => new Promise(r => setTimeout(r, ms));

// Last live data, replayed when the page finishes loading (events can arrive
// before the UI has registered its listeners).
const lastLive = { network: null, clients: null, health: null, news: null, discord: null, profile: null, update: null, matches: {} };

function send(channel, data) {
  if (channel === "network-status") lastLive.network = data;
  else if (channel === "clients-update") lastLive.clients = data;
  else if (channel === "health-status") lastLive.health = data;
  else if (channel === "news-update") lastLive.news = data;
  else if (channel === "discord-count") lastLive.discord = data;
  else if (channel === "profile-update") lastLive.profile = data;
  else if (channel === "update-status") lastLive.update = data;
  else if (channel === "match-event" && data && typeof data === "object") {
    if (data.mode !== undefined) lastLive.matches[String(data.mode)] = data;
    else if (data.amount !== undefined) lastLive.clients = { ok: true, amount: Number(data.amount) };
  }
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(channel, data);
  }
}

function replayLive() {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  const wc = mainWindow.webContents;
  if (lastLive.network) wc.send("network-status", lastLive.network);
  if (lastLive.clients) wc.send("clients-update", lastLive.clients);
  if (lastLive.health) wc.send("health-status", lastLive.health);
  if (lastLive.news) wc.send("news-update", lastLive.news);
  if (lastLive.discord) wc.send("discord-count", lastLive.discord);
  if (lastLive.profile) wc.send("profile-update", lastLive.profile);
  if (lastLive.update) wc.send("update-status", lastLive.update);
  for (const ev of Object.values(lastLive.matches)) wc.send("match-event", ev);
}

function freeBytes(dir) {
  try {
    const s = fs.statfsSync(dir);
    return Number(s.bavail) * Number(s.bsize);
  } catch {
    return Infinity;
  }
}

const gb = n => (n / 1024 ** 3).toFixed(1) + " GB";

function requestJson(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { headers: { "User-Agent": USER_AGENT } }, res => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", chunk => (body += chunk));
      res.on("end", () => {
        if (res.statusCode < 200 || res.statusCode >= 300) {
          return reject(new Error(`HTTP ${res.statusCode}`));
        }
        try { resolve(JSON.parse(body)); }
        catch { reject(new Error("Invalid JSON response")); }
      });
    });
    req.setTimeout(10000, () => req.destroy(new Error("Request timeout")));
    req.on("error", reject);
  });
}

/* ------------------------------------------------------------------ */
/* Live network data (unchanged behaviour)                             */
/* ------------------------------------------------------------------ */


/* ------------------------------------------------------------------ */
/* Server health + news feed                                           */
/* ------------------------------------------------------------------ */

function pollHealth() {
  const req = https.get(HEALTH_URL, { headers: { "User-Agent": USER_AGENT } }, res => {
    send("health-status", { ok: res.statusCode >= 200 && res.statusCode < 300 });
    res.resume();
  });
  req.setTimeout(6000, () => req.destroy(new Error("health timeout")));
  req.on("error", () => send("health-status", { ok: false }));
}

function newsCachePath() { return path.join(app.getPath("userData"), "news-cache.json"); }

function sanitizeNews(raw) {
  if (!raw || !Array.isArray(raw.posts)) return null;
  const posts = raw.posts.filter(p => p && typeof p.id === "string" && p.title && typeof p.title === "object")
    .map(p => ({ id: p.id, date: String(p.date || ""), title: p.title, summary: p.summary || {}, url: typeof p.url === "string" ? p.url : "" }));
  return { updated: String(raw.updated || ""), posts };
}

function fetchNews() {
  const req = https.get(NEWS_URL, { headers: { "User-Agent": USER_AGENT } }, res => {
    let body = "";
    res.setEncoding("utf8");
    res.on("data", c => { if (body.length < 500000) body += c; });
    res.on("end", () => {
      try {
        const news = res.statusCode === 200 ? sanitizeNews(JSON.parse(body)) : null;
        if (!news) throw new Error("bad news");
        fs.writeFileSync(newsCachePath(), JSON.stringify(news));
        send("news-update", news);
      } catch { loadCachedNews(); }
    });
  });
  req.setTimeout(8000, () => req.destroy(new Error("news timeout")));
  req.on("error", loadCachedNews);
}

function loadCachedNews() {
  if (lastLive.news) return;
  try { send("news-update", sanitizeNews(JSON.parse(fs.readFileSync(newsCachePath(), "utf8")))); } catch {}
}

function fetchDiscord() {
  const url = "https://discord.com/api/v9/invites/" + encodeURIComponent(ENDPOINTS.discordInvite || "") + "?with_counts=true";
  const req = https.get(url, { headers: { "User-Agent": USER_AGENT } }, res => {
    let body = "";
    res.setEncoding("utf8");
    res.on("data", c => { if (body.length < 200000) body += c; });
    res.on("end", () => {
      try {
        const j = JSON.parse(body);
        if (typeof j.approximate_member_count === "number") {
          send("discord-count", { members: j.approximate_member_count, online: j.approximate_presence_count });
        }
      } catch {}
    });
  });
  req.setTimeout(8000, () => req.destroy());
  req.on("error", () => {});
}

// Asks the Project Legacy API for the player's display name (shown on the Dashboard).
// Tries "profileUrl" from endpoints.json first (use {email}), then the standard account lookup.
async function fetchProfile() {
  if (!config.email) return;
  const email = encodeURIComponent(config.email);
  const api = String(ENDPOINTS.apiUrl || "").replace(/\/+$/, "");
  const urls = [];
  if (String(ENDPOINTS.profileUrl || "").startsWith("https://")) urls.push(String(ENDPOINTS.profileUrl).replace("{email}", email));
  if (api.startsWith("https://")) urls.push(api + "/account/api/public/account/email/" + email);

  for (const url of urls) {
    try {
      const j = await requestJson(url);
      const name = j && (j.displayName || j.username || j.name);
      if (typeof name === "string" && name.trim() && name.length <= 40) {
        log("profile: ok via " + url.replace(email, "<email>"));
        send("profile-update", { name: name.trim() });
        return;
      }
      log("profile: no displayName in answer from " + url.replace(email, "<email>"));
    } catch (err) {
      log("profile: " + err.message + " for " + url.replace(email, "<email>"));
    }
  }
}

/* ------------------------------------------------------------------ */
/* Self-healing: backup of bin/, diagnostics, "Fix my game"            */
/* ------------------------------------------------------------------ */

const BIN_FILES = ["Injector.exe", "projectlegacy.dll", "edit.dll"];
const binBackupDir = () => path.join(app.getPath("userData"), "bin-backup");

function backupBin() {
  try {
    fs.mkdirSync(binBackupDir(), { recursive: true });
    for (const f of BIN_FILES) {
      const src = path.join(BIN_DIR, f), dst = path.join(binBackupDir(), f);
      if (fs.existsSync(src) && !fs.existsSync(dst)) fs.copyFileSync(src, dst);
    }
  } catch {}
}

function restoreBin() {
  const restored = [];
  for (const f of BIN_FILES) {
    const dst = path.join(BIN_DIR, f), src = path.join(binBackupDir(), f);
    try {
      if (!fs.existsSync(dst) && fs.existsSync(src)) { fs.copyFileSync(src, dst); restored.push(f); }
    } catch {}
  }
  if (restored.length) log("restored from backup: " + restored.join(", "));
  return restored;
}

function runningProcesses() {
  return new Promise(resolve => {
    if (process.platform !== "win32") return resolve([]);
    execFile("tasklist", ["/FO", "CSV", "/NH"], { windowsHide: true }, (err, out) => {
      if (err) return resolve([]);
      resolve(String(out).split("\n").map(l => (l.split('","')[0] || "").replace(/"/g, "").toLowerCase()));
    });
  });
}

function findGameCopies(base) {
  const target = path.basename(GAME_EXE_REL).toLowerCase();
  const skip = ["content", "engine", "paks", "movies", "splash"];
  const found = [];
  (function walk(dir, depth) {
    if (depth > 7 || found.length > 5) return;
    let entries;
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      if (e.isDirectory()) { if (!skip.includes(e.name.toLowerCase())) walk(path.join(dir, e.name), depth + 1); }
      else if (e.name.toLowerCase() === target) found.push(dir);
    }
  })(base, 0);
  return found;
}

function findLeftovers(base) {
  const found = [];
  try {
    for (const f of fs.readdirSync(base)) if (/\.(rar|part)$/i.test(f)) found.push(f);
  } catch {}
  return found;
}

async function runDiagnostics(fix) {
  const checks = [];
  const add = (id, label, ok, detail, fixed) => checks.push({ id, label, ok, detail: detail || "", fixed: Boolean(fixed) });

  // 1. game folder + files
  const folderOk = Boolean(config.gameFolder && fs.existsSync(config.gameFolder));
  add("folder", "Game folder", folderOk, folderOk ? config.gameFolder : "Not set or not found. Choose it in Settings.");
  const root = folderOk ? installedRoot() : null;
  add("exe", "Game executable", Boolean(root), root ? "" : "Missing. Use the Dashboard to download the build again.");
  let paksOk = false;
  if (root) {
    try { paksOk = fs.readdirSync(path.join(root, "FortniteGame", "Content", "Paks")).some(f => f.toLowerCase().endsWith(".pak")); } catch {}
  }
  add("paks", "Game data (.pak files)", paksOk, paksOk ? "" : "Missing or incomplete. Re-download the build from the Dashboard.");

  // 1b. several copies of the game / unfinished extraction (a mixed install crashes at startup)
  if (folderOk) {
    const copies = findGameCopies(config.gameFolder);
    add("copies", "Only one copy of the game", copies.length <= 1,
      copies.length > 1 ? "Found " + copies.length + " copies: " + copies.slice(0, 3).join(" | ") + ". Keep one, or choose an empty folder and use REINSTALL / REPAIR." : "");
    const left = findLeftovers(config.gameFolder);
    add("leftover", "Extraction finished", !left.length,
      left.length ? "Leftover " + left.join(", ") + ": the download or extraction did not finish. Use REINSTALL / REPAIR in Downloads (in an empty folder)." : "");
  }

  // 2. launcher tools (antivirus may remove them)
  let restored = [];
  if (fix) restored = restoreBin();
  const missingBin = BIN_FILES.filter(f => !fs.existsSync(path.join(BIN_DIR, f)));
  add("bin", "Injector and DLLs", !missingBin.length,
    missingBin.length ? missingBin.join(", ") + " missing. Restore them in Windows Security > Protection history, then add the launcher folder to the exclusions." : (restored.length ? "Restored: " + restored.join(", ") : ""),
    restored.length > 0);

  // 3. launch arguments
  let argsOk = /-HTTP=WinINet/i.test(config.extraArgs || "");
  let argsFixed = false;
  if (!argsOk && fix) { config.extraArgs = DEFAULT_ARGS; saveConfig(); argsOk = true; argsFixed = true; }
  add("args", "Launch arguments", argsOk, argsOk ? "" : "-HTTP=WinINet is missing. It is required by the redirector.", argsFixed);

  // 4. disk space
  if (folderOk) {
    try {
      const s = fs.statfsSync(config.gameFolder);
      const freeGb = (Number(s.bavail) * Number(s.bsize)) / 1024 ** 3;
      add("disk", "Free disk space", freeGb >= 2, freeGb.toFixed(1) + " GB free" + (freeGb < 2 ? ". Free some space and try again." : ""));
    } catch {}
  }

  // 5. Visual C++ runtime
  if (process.platform === "win32") {
    const sys32 = path.join(process.env.SystemRoot || "C:\\Windows", "System32");
    const vc = fs.existsSync(path.join(sys32, "vcruntime140.dll")) && fs.existsSync(path.join(sys32, "msvcp140.dll"));
    add("vcredist", "Visual C++ runtime", vc, vc ? "" : "Missing. Install the Microsoft Visual C++ Redistributable (x64).");
  }

  // 6. leftover game processes (a stuck game blocks the next launch)
  if (game.state === "idle") {
    const names = [path.basename(GAME_EXE_REL), ...STUB_EXES].map(n => n.toLowerCase());
    const stuck = (await runningProcesses()).filter(p => names.includes(p));
    let killed = false;
    if (stuck.length && fix) {
      for (const n of [...new Set(stuck)]) await new Promise(r => execFile("taskkill", ["/F", "/IM", n], { windowsHide: true }, () => r()));
      killed = true;
    }
    add("procs", "Stuck game processes", !stuck.length || killed, stuck.length ? (killed ? "Closed: " : "Found: ") + [...new Set(stuck)].join(", ") : "", killed);
  }

  // 7. server
  const healthy = !lastLive.health || lastLive.health.ok;
  add("server", "Project Legacy server", healthy, healthy ? "" : "The server does not answer. Wait a few minutes, it is not a problem on your PC.");

  // 8. injection delay learned from crashes
  if (fix && config.injectDelayMs !== 2500) { /* kept on purpose: it was raised by a startup crash */ }

  log("diagnostics: " + checks.filter(c => !c.ok).map(c => c.id).join(",") + " problem(s)");
  return { checks, delayMs: config.injectDelayMs };
}

ipcMain.handle("reset-game-settings", () => {
  try {
    const dir = path.join(process.env.LOCALAPPDATA || "", "FortniteGame", "Saved", "Config");
    if (!process.env.LOCALAPPDATA || !fs.existsSync(dir)) return { ok: true, message: "Nothing to reset: the game has no saved settings yet." };
    const backup = dir + ".bak-" + Date.now();
    fs.renameSync(dir, backup);
    log("game settings reset (old folder kept as " + path.basename(backup) + ")");
    return { ok: true, message: "Game settings reset. The old folder was kept as " + path.basename(backup) + " next to it. Press PLAY to try again." };
  } catch (err) {
    return { ok: false, message: "Could not reset the settings: " + err.message };
  }
});

ipcMain.handle("get-compat", () => config.compat);
ipcMain.handle("set-compat", (_e, on) => { config.compat = Boolean(on); saveConfig(); return config.compat; });

ipcMain.handle("run-diagnostics", (_e, fix) => runDiagnostics(Boolean(fix)));

// Continuous self-healing: puts back tools removed by the antivirus, every minute.
function startHealLoop() {
  backupBin();
  setInterval(() => { if (game.state === "idle") restoreBin(); }, 60000);
}

function startBackgroundServices() {
  startHealLoop();
  fetchProfile();
  fetchDiscord();
  setInterval(fetchDiscord, 30 * 60 * 1000);
  // Launcher self-update (installed NSIS build only; skipped for npm start and the portable .exe)
  try {
    const pub = ((require("./package.json").build || {}).publish || [])[0];
    const updaterReady = Boolean(pub) && !/REMPLACE/i.test(JSON.stringify(pub));
    if (app.isPackaged && !process.env.PORTABLE_EXECUTABLE_DIR && updaterReady) {
      const { autoUpdater } = require("electron-updater");
      autoUpdater.autoDownload = true;
      autoUpdater.on("error", err => log("updater: " + String(err.message).split("\n")[0].slice(0, 160)));
      autoUpdater.on("update-available", info => send("update-status", { state: "downloading", version: info.version, percent: 0 }));
      autoUpdater.on("download-progress", p => send("update-status", { state: "downloading", percent: Math.round(p.percent) }));
      autoUpdater.on("update-downloaded", info => send("update-status", { state: "ready", version: info.version }));
      ipcMain.handle("update-install", () => autoUpdater.quitAndInstall());
      const check = () => autoUpdater.checkForUpdates().catch(err => log("updater: " + err.message));
      check();
      setInterval(check, 4 * 60 * 60 * 1000);
    }
  } catch {}
  pollHealth();
  setInterval(pollHealth, Math.max(10, Number(ENDPOINTS.healthPollSeconds) || 20) * 1000);
  fetchNews();
  setInterval(fetchNews, 12 * 60 * 1000);
}

function connectEvents() {
  if (eventRequest) eventRequest.destroy();

  const req = https.get(EVENTS_URL, {
    headers: { Accept: "text/event-stream", "Cache-Control": "no-cache" }
  }, res => {
    if (res.statusCode !== 200) {
      send("network-status", { events: false });
      res.resume();
      setTimeout(connectEvents, 8000);
      return;
    }

    send("network-status", { events: true });
    res.setEncoding("utf8");
    let buffer = "";

    res.on("data", chunk => {
      buffer += chunk;
      buffer = buffer.replace(/\r\n/g, "\n");
      let boundary;
      while ((boundary = buffer.indexOf("\n\n")) !== -1) {
        const block = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);

        let eventName = "message";
        const dataLines = [];
        for (const line of block.split(/\r?\n/)) {
          if (line.startsWith("event:")) eventName = line.slice(6).trim();
          if (line.startsWith("data:")) dataLines.push(line.slice(5).trim());
        }

        if (dataLines.length && (eventName === "match" || eventName === "message")) {
          try { send("match-event", JSON.parse(dataLines.join("\n"))); } catch {}
        }
      }
    });

    res.on("end", () => {
      send("network-status", { events: false });
      setTimeout(connectEvents, 5000);
    });
  });

  // The server sends a ping regularly; if nothing arrives for 60s, reconnect.
  req.setTimeout(60000, () => req.destroy(new Error("events timeout")));

  req.on("error", () => {
    send("network-status", { events: false });
    setTimeout(connectEvents, 8000);
  });
  eventRequest = req;
}

async function updateClients() {
  try {
    const data = await requestJson(CLIENTS_URL);
    const amount = Number(data.amount);
    const clients = Array.isArray(data.clients)
      ? data.clients.filter(c => typeof c === "string").map(c => c.trim().slice(0, 40)).filter(Boolean).slice(0, 100)
      : null;
    send("clients-update", {
      ok: Number.isFinite(amount),
      amount: Number.isFinite(amount) ? amount : null,
      clients
    });
  } catch {
    send("clients-update", { ok: false, amount: null });
  }
}

/* ------------------------------------------------------------------ */
/* Install: download + extract                                         */
/* ------------------------------------------------------------------ */

const LOG = [];
function log(msg) {
  LOG.push(new Date().toISOString() + "  " + msg);
  if (LOG.length > 400) LOG.shift();
}

function setInstall(patch, replace = false) {
  if (patch.message || (patch.phase && patch.phase !== install.phase)) log("install: " + (patch.phase || install.phase) + " " + (patch.message || ""));
  install = replace ? patch : { ...install, ...patch };
  send("install-progress", install);
}

function extractArchive(archive, target) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(path.join(__dirname, "extract-worker.js"), {
      workerData: { archive, target }
    });
    extractWorker = worker;
    const startedAt = Date.now();
    let finished = false;
    const end = (fn, value) => {
      if (finished) return;
      finished = true;
      extractWorker = null;
      fn(value);
    };

    worker.on("message", m => {
      if (m.type === "progress") {
        const elapsed = Math.max(1, Date.now() - startedAt) / 1000;
        const speed = m.bytesDone / elapsed;
        setInstall({
          phase: "extracting",
          percent: m.bytesTotal ? (m.bytesDone / m.bytesTotal) * 100 : 0,
          files: m.total,
          filesDone: m.done,
          bytesDone: m.bytesDone,
          bytesTotal: m.bytesTotal,
          speed,
          eta: speed > 1 ? (m.bytesTotal - m.bytesDone) / speed : null,
          currentFile: m.name,
          message: ""
        });
      } else if (m.type === "done") {
        end(resolve);
      } else if (m.type === "error") {
        end(reject, new Error(m.message));
      }
    });
    worker.on("error", err => end(reject, err));
    worker.on("exit", code => {
      if (cancelRequested) end(reject, Object.assign(new Error("cancelled"), { aborted: "cancel" }));
      else end(reject, new Error(`Extraction stopped unexpectedly (code ${code}).`));
    });
  });
}

async function runInstall() {
  if (installBusy) return;
  if (!config.gameFolder) {
    setInstall({ phase: "error", message: "Choose an installation folder first." }, true);
    return;
  }

  installBusy = true;
  cancelRequested = false;
  const workDir = path.join(config.gameFolder, WORK_DIR_NAME);
  const archive = path.join(workDir, ARCHIVE_NAME);
  const part = archive + ".part";

  try {
    fs.mkdirSync(workDir, { recursive: true });

    // 1. Download (resumes from the .part file if there is one)
    if (!fs.existsSync(archive)) {
      let spaceChecked = false;
      setInstall({ phase: "downloading", percent: 0, received: 0, total: 0, speed: 0, eta: null, message: "" }, true);

      downloadHandle = download({
        url: BUILD_URL,
        partPath: part,
        userAgent: USER_AGENT,
        onProgress: p => {
          if (p.retrying) {
            setInstall({ message: `Connection problem, retrying (${p.retrying}/5)...` });
            return;
          }
          if (!spaceChecked && p.total > 0) {
            spaceChecked = true;
            // Archive + the extracted copy both live on disk at the same time.
            const needed = (p.total - p.received) + p.total * 1.2;
            if (freeBytes(config.gameFolder) < needed) {
              setInstall({ message: `Low disk space: about ${gb(needed)} needed.` });
            }
          }
          setInstall({
            phase: "downloading",
            percent: p.total ? (p.received / p.total) * 100 : 0,
            received: p.received,
            total: p.total,
            speed: p.speed,
            eta: p.eta,
            message: ""
          });
        }
      });
      await downloadHandle.promise;
      downloadHandle = null;
      fs.renameSync(part, archive);
    }

    // 1b. Verify SHA-256 when the expected hash is set in endpoints.json
    const expected = String(ENDPOINTS.buildArchiveSha256 || "").trim().toLowerCase();
    if (expected) {
      setInstall({ phase: "extracting", percent: 0, speed: 0, eta: null, message: "Verifying download (SHA-256)..." }, true);
      const actual = await new Promise((resolve, reject) => {
        const h = require("crypto").createHash("sha256");
        fs.createReadStream(archive).on("data", c => h.update(c)).on("end", () => resolve(h.digest("hex"))).on("error", reject);
      });
      if (actual !== expected) {
        fs.rmSync(archive, { force: true });
        log("sha256 mismatch: " + actual);
        throw new Error("The downloaded file is corrupted (SHA-256 does not match). It was deleted - press Download to try again.");
      }
    }

    // 2. Extract (in a worker thread)
    setInstall({ phase: "extracting", percent: 0, speed: 0, eta: null, message: "Reading the archive..." }, true);
    fs.writeFileSync(incompleteMarker(), "extraction in progress\n");
    await extractArchive(archive, config.gameFolder);

    // 3. Check the result
    if (!findBuildRoot(config.gameFolder)) {
      throw new Error("Extraction finished but the game executable was not found. The archive layout may have changed.");
    }
    try { fs.rmSync(incompleteMarker(), { force: true }); } catch {}
    try { fs.rmSync(archive, { force: true }); } catch {}
    setInstall({ phase: "done", percent: 100, message: "Installation complete." }, true);
  } catch (err) {
    downloadHandle = null;
    if (err.aborted === "pause") {
      setInstall({ phase: "paused", message: "Download paused." });
    } else if (err.aborted === "cancel") {
      try { fs.rmSync(part, { force: true }); } catch {}
      setInstall({ phase: "idle", message: "Cancelled." }, true);
    } else {
      setInstall({ phase: "error", message: err.message || String(err) }, true);
    }
  } finally {
    installBusy = false;
  }
}

/* ------------------------------------------------------------------ */
/* Game launch + DLL injection (Windows only)                          */
/* ------------------------------------------------------------------ */

function setGame(patch) {
  game = { ...game, ...patch };
  send("game-status", { state: game.state, message: game.message });
}

function suspendPid(pid) {
  // Freezes a process with ntdll!NtSuspendProcess via PowerShell.
  const script =
    "$s='[DllImport(\"ntdll.dll\")] public static extern int NtSuspendProcess(IntPtr h);';" +
    "$t=Add-Type -MemberDefinition $s -Name N -Namespace W -PassThru;" +
    `$p=[System.Diagnostics.Process]::GetProcessById(${Number(pid)});` +
    "[void]$t::NtSuspendProcess($p.Handle)";
  return new Promise(resolve => {
    execFile("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script],
      { windowsHide: true, timeout: 15000 }, () => resolve());
  });
}

function runInjector(pid, dllPath) {
  return new Promise(resolve => {
    execFile(INJECTOR, ["-p", String(pid), "-i", dllPath],
      { windowsHide: true, timeout: 20000 }, (err, stdout, stderr) => {
        const output = `${stdout || ""}${stderr || ""}`.trim();
        resolve({ ok: !err && /successfully injected/i.test(output), output: output || (err && err.message) || "" });
      });
  });
}

function killTree(pid) {
  execFile("taskkill", ["/PID", String(pid), "/T", "/F"], { windowsHide: true }, () => {});
}

function cleanupStubs() {
  for (const stub of game.stubs) {
    try { process.kill(stub.pid); } catch {}
  }
  game.stubs = [];
}

function verifyFiles() {
  const lines = [];
  let ok = true;
  const check = (label, file) => {
    const exists = fs.existsSync(file);
    if (!exists) ok = false;
    lines.push({ label, ok: exists });
  };
  check("Injector.exe", INJECTOR);
  for (const inj of INJECTIONS) check(inj.file, path.join(BIN_DIR, inj.file));
  for (const mod of Object.values(MANUAL_MODULES)) {
    const exists = fs.existsSync(path.join(BIN_DIR, mod.file));
    if (!exists && !mod.optional) ok = false;
    lines.push({ label: mod.file + (mod.optional ? " (optional)" : ""), ok: exists });
  }
  const root = installedRoot();
  lines.push({ label: "Game build (FortniteClient-Win64-Shipping.exe)", ok: Boolean(root) });
  if (!root) ok = false;
  return { ok, lines };
}

async function launchGame() {
  if (game.state !== "idle") return { ok: false, message: "The game is already running." };
  if (process.platform !== "win32") return { ok: false, message: "The game can only be launched on Windows." };
  if (installBusy) return { ok: false, message: "Wait for the installation to finish." };

  const root = installedRoot();
  if (!root) return { ok: false, message: "Game build not found or incomplete. Install it or choose the right folder in Settings." };

  const password = readPassword();
  if (!config.email || !password) {
    return { ok: false, needAccount: true, message: "Enter your account email and password in Settings first." };
  }

  const missing = [INJECTOR, ...INJECTIONS.map(i => path.join(BIN_DIR, i.file))].filter(f => !fs.existsSync(f));
  if (missing.length) {
    return { ok: false, message: `Missing launcher files: ${missing.map(f => path.basename(f)).join(", ")}. A security tool may have removed them.` };
  }

  const binaries = path.join(root, path.dirname(GAME_EXE_REL));
  const gameExe = path.join(root, GAME_EXE_REL);

  setGame({ state: "launching", message: "Starting the game..." });

  try {
    // Placeholder processes, started first and frozen.
    if (config.useStubs) {
      for (const name of STUB_EXES) {
        const exe = path.join(binaries, name);
        if (!fs.existsSync(exe)) continue;
        const child = spawn(exe, [], { cwd: binaries, stdio: "ignore", windowsHide: true });
        child.on("error", () => {});
        if (child.pid) {
          game.stubs.push({ pid: child.pid, name });
          await suspendPid(child.pid);
        }
      }
    }

    const args = buildLaunchArgs({ extraArgs: config.extraArgs, email: config.email, password });
    const child = spawn(gameExe, args, { cwd: binaries, stdio: "ignore" });
    const startedAt = Date.now();

    await new Promise((resolve, reject) => {
      child.once("spawn", resolve);
      child.once("error", reject);
    });

    game.pid = child.pid;
    if (config.compat && process.platform === "win32") {
      // Compatibility mode: keep the game on the first 8 logical processors (the performance cores of
      // hybrid Intel CPUs). Old Unreal Engine 4 builds sometimes crash when scheduled on efficiency cores.
      execFile("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command",
        `try { (Get-Process -Id ${child.pid}).ProcessorAffinity = [IntPtr]255 } catch {}`], { windowsHide: true }, () => {});
      log("compatibility mode: game limited to the first 8 logical processors");
    }
    setGame({ state: "running", message: "Game started, injecting modules..." });

    child.on("exit", code => {
      cleanupStubs();
      game.pid = null;
      const runtime = Date.now() - startedAt;
      const earlyCrash = runtime < 120000;
      let message = code ? `The game closed (exit code ${code}).` : "The game closed.";
      let retry = false;

      if (code === 3221225477 && earlyCrash) {
        const delay = config.injectDelayMs;
        const injected = runtime > delay + 800;
        log(`game crashed after ${Math.round(runtime / 1000)}s (injection planned at ${delay} ms, injected: ${injected})`);
        let next = delay;
        if (!injected && delay > 1000) next = Math.max(500, Math.round(runtime / 2));      // crashed BEFORE injecting: inject earlier
        else if (injected && runtime - delay < 6000 && delay < 15000) next = Math.min(delay + 3000, 15000); // crashed right AFTER injecting: inject later
        const sameMoment = lastCrash && Math.abs(lastCrash.runtime - runtime) < 2500 && lastCrash.delay !== delay;
        lastCrash = { runtime, delay };
        if (sameMoment) log("crash happens at the same moment whatever the injection timing: injection is not the cause");
        if (!sameMoment && next !== delay && autoRetries < 3) {
          config.injectDelayMs = next;
          saveConfig();
          retry = true;
          message = "The game crashed at startup. Trying again with a different injection timing (" + (autoRetries + 1) + "/3)...";
        } else {
          message = "The game keeps crashing at startup, and the injection timing is not the cause. This usually comes from the PC: update your graphics drivers, " +
            "close overlays (Discord, GeForce Experience, MSI Afterburner), then open Help: RESET GAME SETTINGS, then try COMPATIBILITY MODE. If it continues, send COPY LOGS (Settings) to support.";
        }
      } else if (code === 3221225781) {
        message = "The game could not start: a Windows component is missing. Install the Microsoft Visual C++ Redistributable (x64), then press PLAY again.";
      }

      if (!earlyCrash) autoRetries = 0;
      setGame({ state: "idle", message });
      if (retry) {
        autoRetries++;
        log("auto-retry " + autoRetries + "/3 with injection at " + config.injectDelayMs + " ms");
        setTimeout(() => { if (game.state === "idle") launchGame(); }, 3000);
      } else if (code === 3221225477 && earlyCrash) {
        autoRetries = 0;
      }
    });

    // Inject in the background so the UI stays responsive.
    (async () => {
      const failures = [];
      for (const inj of INJECTIONS) {
        const wait = Math.max(inj.delayMs, config.injectDelayMs) - (Date.now() - startedAt);
        if (wait > 0) await sleep(wait);
        if (game.pid !== child.pid) return; // game already closed
        const result = await runInjector(child.pid, path.join(BIN_DIR, inj.file));
        if (!result.ok) failures.push(`${inj.label}: ${result.output || "failed"}`);
      }
      if (game.pid !== child.pid) return;
      setGame({
        message: failures.length
          ? `Game running, but injection failed (${failures.join(" | ")})`
          : "Game running, redirector loaded. In the lobby, use INJECT CONSOLE below (then press F3 in-game)."
      });
    })();

    return { ok: true };
  } catch (err) {
    cleanupStubs();
    game.pid = null;
    setGame({ state: "idle", message: `Could not start the game: ${err.message}` });
    return { ok: false, message: `Could not start the game: ${err.message}` };
  }
}

/* ------------------------------------------------------------------ */
/* Window                                                              */
/* ------------------------------------------------------------------ */

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 1280,
    minHeight: 720,
    backgroundColor: "#090711",
    title: "Project Legacy Launcher",
    icon: path.join(__dirname, "assets", "logo.png"),
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  });

  mainWindow.loadFile("index.html");
  mainWindow.webContents.on("did-finish-load", () => setTimeout(replayLive, 300));
  mainWindow.on("closed", () => (mainWindow = null));

  // Never navigate away from the launcher UI; open web links in the real browser.
  mainWindow.webContents.on("will-navigate", e => e.preventDefault());
  mainWindow.webContents.setWindowOpenHandler(() => ({ action: "deny" }));

  connectEvents();
  startBackgroundServices();
  updateClients();
  const clientsTimer = setInterval(() => {
    if (!mainWindow) clearInterval(clientsTimer);
    else updateClients();
  }, 30000);
}

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on("second-instance", () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
  });

  app.whenReady().then(() => {
    loadConfig();
    createWindow();
    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  });
}

app.on("before-quit", () => {
  if (eventRequest) eventRequest.destroy();
  if (downloadHandle) downloadHandle.abort("quit"); // keeps the .part file for next time
  if (extractWorker) extractWorker.terminate();
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

/* ------------------------------------------------------------------ */
/* IPC                                                                 */
/* ------------------------------------------------------------------ */

ipcMain.handle("get-config", () => publicConfig());

ipcMain.handle("get-state", () => ({
  install,
  game: { state: game.state, message: game.message }
}));

ipcMain.handle("choose-folder", async () => {
  if (installBusy) return null;
  const result = await dialog.showOpenDialog(mainWindow, {
    title: "Choose your game folder",
    properties: ["openDirectory", "createDirectory"]
  });
  if (result.canceled || !result.filePaths[0]) return null;

  config.gameFolder = result.filePaths[0];
  saveConfig();
  if (install.phase === "done" || install.phase === "error") setInstall({ phase: "idle" }, true);
  return config.gameFolder;
});

ipcMain.handle("check-folder", async () => {
  if (!config.gameFolder) return { ok: false, installed: false, message: "Choose a folder first." };
  try {
    const entries = fs.readdirSync(config.gameFolder);
    const root = installedRoot();
    if (root) return { ok: true, installed: true, message: `Build found: ${root}` };
    if (findBuildRoot(config.gameFolder)) {
      return { ok: true, installed: false, message: "A previous installation was interrupted. Use Download to finish it." };
    }
    return {
      ok: true,
      installed: false,
      message: entries.length
        ? "Folder accessible, but no game build found in it."
        : "Folder is empty. Use Download to install the build here."
    };
  } catch {
    return { ok: false, installed: false, message: "The selected folder cannot be accessed." };
  }
});

ipcMain.handle("open-folder", async () => {
  if (!config.gameFolder) return false;
  const error = await shell.openPath(config.gameFolder);
  return !error;
});

ipcMain.handle("save-account", (_event, data) => {
  const email = typeof data?.email === "string" ? data.email.trim() : "";
  const password = typeof data?.password === "string" ? data.password : "";
  if (email.length > 200 || password.length > 200 || /[\r\n]/.test(email + password)) {
    return { ok: false, message: "Invalid email or password." };
  }
  config.email = email;
  if (password) storePassword(password); // empty password field = keep the saved one
  if (!email) storePassword("");
  saveConfig();
  fetchProfile();
  return { ok: true, hasPassword: Boolean(config.passwordEnc) };
});

ipcMain.handle("save-launch-options", (_event, data) => {
  const extraArgs = typeof data?.extraArgs === "string" ? data.extraArgs.trim() : DEFAULT_ARGS;
  if (extraArgs.length > 1000 || /[\r\n]/.test(extraArgs)) {
    return { ok: false, message: "Launch arguments are invalid." };
  }
  config.extraArgs = extraArgs || DEFAULT_ARGS;
  config.useStubs = Boolean(data?.useStubs);
  saveConfig();
  return { ok: true, extraArgs: config.extraArgs };
});

ipcMain.handle("verify-files", () => verifyFiles());

ipcMain.handle("install-start", () => {
  if (installBusy) return { ok: false, message: "Already running." };
  runInstall(); // runs in the background, progress arrives through events
  return { ok: true };
});

ipcMain.handle("install-pause", () => {
  if (downloadHandle) downloadHandle.abort("pause");
  return true;
});

ipcMain.handle("install-cancel", () => {
  cancelRequested = true;
  if (downloadHandle) {
    downloadHandle.abort("cancel");
  } else if (extractWorker) {
    extractWorker.terminate();
  } else if (!installBusy) {
    // paused: nothing is running, just drop the partial file
    try { fs.rmSync(path.join(config.gameFolder, WORK_DIR_NAME, ARCHIVE_NAME + ".part"), { force: true }); } catch {}
    setInstall({ phase: "idle" }, true);
  }
  return true;
});

ipcMain.handle("game-launch", () => { autoRetries = 0; return launchGame(); });

ipcMain.handle("game-inject", async (_event, key) => {
  const mod = MANUAL_MODULES[key];
  if (!mod) return { ok: false, message: "Unknown module." };
  if (game.state !== "running" || !game.pid) return { ok: false, message: "Start the game first." };
  const file = path.join(BIN_DIR, mod.file);
  if (!fs.existsSync(file)) {
    return { ok: false, message: `${mod.file} is missing from the launcher's bin folder.` };
  }
  const result = await runInjector(game.pid, file);
  const message = result.ok
    ? (key === "console" ? "Console injected. Press F3 in-game." : `${mod.label} injected.`)
    : `${mod.label} injection failed: ${result.output || "unknown error"}`;
  setGame({ message });
  return { ok: result.ok, message };
});

ipcMain.handle("game-stop", () => {
  if (game.pid) killTree(game.pid);
  cleanupStubs();
  return true;
});

ipcMain.handle("list-backgrounds", async () => {
  try {
    const dir = path.join(__dirname, "assets", "backgrounds");
    const mime = { ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".png": "image/png", ".webp": "image/webp" };
    return fs.readdirSync(dir)
      .filter(f => mime[path.extname(f).toLowerCase()])
      .sort()
      .slice(0, 20)
      .map(f => "data:" + mime[path.extname(f).toLowerCase()] + ";base64," +
        fs.readFileSync(path.join(dir, f)).toString("base64"));
  } catch {
    return [];
  }
});

ipcMain.handle("preflight", () => {
  const problems = [];
  let repair = false;
  if (!config.gameFolder || !fs.existsSync(config.gameFolder)) {
    return { ok: false, problems: ["The game folder cannot be found. Choose it again in Settings."], repair: false };
  }
  const root = installedRoot();
  if (!root) {
    problems.push("The game executable is missing.");
    repair = true;
  } else {
    try {
      const paks = path.join(root, "FortniteGame", "Content", "Paks");
      const count = fs.readdirSync(paks).filter(f => f.toLowerCase().endsWith(".pak")).length;
      if (!count) { problems.push("The game data (.pak files) is missing."); repair = true; }
    } catch {
      problems.push("The game data folder (Paks) is missing.");
      repair = true;
    }
  }
  for (const f of ["Injector.exe", "projectlegacy.dll"]) {
    if (!fs.existsSync(path.join(BIN_DIR, f))) {
      problems.push(f + " is missing - your antivirus probably removed it. Restore it in Windows Security > Protection history.");
    }
  }
  if (problems.length) log("preflight: " + problems.join(" | "));
  return { ok: !problems.length, problems, repair };
});

ipcMain.handle("get-logs", async () => {
  const os = require("os");
  const out = [];
  try {
    out.push(`system: ${os.cpus()[0].model.trim()}, ${Math.round(os.totalmem() / 1024 ** 3)} GB RAM, Windows ${os.release()}, launcher ${app.getVersion()}, injection delay ${config.injectDelayMs} ms`);
  } catch {}
  out.push(LOG.join("\n") || "No launcher logs yet.");
  try {
    const gameLog = path.join(process.env.LOCALAPPDATA || "", "FortniteGame", "Saved", "Logs", "FortniteGame.log");
    const lines = fs.readFileSync(gameLog, "utf8").split(/\r?\n/).slice(-60).join("\n");
    out.push("--- end of the game log ---\n" + lines.slice(-12000));
  } catch {}
  if (process.platform === "win32") {
    const events = await new Promise(resolve => execFile("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command",
      "Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000} -MaxEvents 40 -ErrorAction SilentlyContinue | Where-Object { $_.Message -match 'Fortnite' } | Select-Object -First 2 | ForEach-Object { $_.TimeCreated.ToString('s') + ' ' + $_.Message.Substring(0,[Math]::Min(450,$_.Message.Length)) }"],
      { windowsHide: true, timeout: 8000 }, (err, stdout) => resolve(err ? "" : String(stdout).trim().slice(0, 3000))));
    if (events) out.push("--- Windows crash events (faulting module) ---\n" + events);
  }
  return out.join("\n");
});

ipcMain.handle("open-link", async (_event, target) => {
  const allowed = [OFFICIAL_SITE, BUILD_URL, DISCORD_URL];
  if (!allowed.includes(target) && !(typeof target === "string" && target.startsWith("https://projectlegacy.pages.dev/")))
    throw new Error("Link not allowed");
  await shell.openExternal(target);
  return true;
});

ipcMain.handle("open-official-site", async () => {
  await shell.openExternal(OFFICIAL_SITE);
  return true;
});
