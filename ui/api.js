// Drop-in replacement for the Electron preload.js. index.html calls
// `launcher.xxx(...)` exactly like before; this file just routes those
// calls to Tauri commands/events instead of ipcRenderer. No other
// frontend file needed to change.
(function () {
  "use strict";
  // Load locally-built UI assets. Tailwind is loaded before the existing theme
  // so Project Legacy's own palette and component overrides remain authoritative.
  const loadProjectLegacyAssets = () => {
    if (!document.getElementById("project-legacy-tailwind")) {
      const tailwind = document.createElement("link");
      tailwind.id = "project-legacy-tailwind";
      tailwind.rel = "stylesheet";
      tailwind.href = new URL("./assets/tailwind.css", window.location.href).toString();
      document.head.appendChild(tailwind);
    }
    if (!document.getElementById("project-legacy-theme")) {
      const theme = document.createElement("link");
      theme.id = "project-legacy-theme";
      theme.rel = "stylesheet";
      theme.href = new URL("./theme.css", window.location.href).toString();
      document.head.appendChild(theme);
    }
    if (!document.getElementById("project-legacy-lucide")) {
      const script = document.createElement("script");
      script.id = "project-legacy-lucide";
      script.src = new URL("./assets/libraries.js", window.location.href).toString();
      script.defer = true;
      document.head.appendChild(script);
    }
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", loadProjectLegacyAssets, { once: true });
  } else {
    loadProjectLegacyAssets();
  }

  // Navigation fallback: the legacy page controller normally handles these
  // clicks. This delegated handler keeps page switching functional if a future
  // frontend script fails before its per-button listeners are registered.
  document.addEventListener("click", (event) => {
    const button = event.target.closest(".nav button[data-page]");
    if (!button) return;

    const name = button.dataset.page;
    const page = document.getElementById("page-" + name);
    if (!page || page.classList.contains("active")) return;

    document.querySelectorAll(".page").forEach((item) => item.classList.remove("active"));
    page.classList.add("active");
    document.querySelectorAll(".nav button[data-page]").forEach((item) => {
      item.classList.toggle("active", item.dataset.page === name);
    });
    const labels = {
      home: "DASHBOARD",
      community: "COMMUNITY",
      profile: "PROFILE",
      shop: "SHOP",
      downloads: "DOWNLOADS",
      settings: "SETTINGS",
    };
    const breadcrumb = document.getElementById("breadcrumbPage");
    if (breadcrumb) breadcrumb.textContent = labels[name] || "DASHBOARD";
    document.body.dataset.page = name;
  });

  const T = window.__TAURI__;
  const invoke = (cmd, args) => T.core.invoke(cmd, args);

  // Mirrors preload's `subscribe`: returns a function synchronously (the
  // real unlisten is wired up as soon as the async `listen()` resolves).
  const subscribe = (channel) => (callback) => {
    let unlisten = null;
    let cancelled = false;
    T.event.listen(channel, (e) => callback(e.payload)).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
      // Tauri events can be emitted before the WebView finishes registering
      // listeners. Replay the latest cached state immediately after subscribing.
      invoke("get_live_snapshot").then((snapshot) => {
        if (cancelled || !snapshot) return;
        if (channel === "match-event") {
          Object.values(snapshot.matches || {}).forEach(callback);
        } else if (snapshot.latest && snapshot.latest[channel]) {
          callback(snapshot.latest[channel]);
        }
      }).catch(() => {});
    }).catch((error) => console.error("Could not subscribe to launcher event", channel, error));
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  };

  window.launcher = {
    // configuration
    getConfig: () => invoke("get_config"),
    completeSetup: () => invoke("complete_setup"),
    chooseFolder: () => invoke("choose_folder"),
    checkFolder: () => invoke("check_folder"),
    openFolder: () => invoke("open_folder"),
    saveAccount: (data) => invoke("save_account", data),
    saveMatchNotifications: (enabled) => invoke("save_match_notifications", { enabled: Boolean(enabled) }),
    requestNotificationPermission: async () => {
      const notifications = T && T.notification;
      if (!notifications || typeof notifications.isPermissionGranted !== "function" ||
          typeof notifications.requestPermission !== "function") return false;
      let granted = await notifications.isPermissionGranted();
      if (!granted) granted = (await notifications.requestPermission()) === "granted";
      return Boolean(granted);
    },
    sendMatchNotification: (body) => {
      const notifications = T && T.notification;
      if (!notifications || typeof notifications.sendNotification !== "function") {
        throw new Error("Windows notifications are not available in this launcher build.");
      }
      notifications.sendNotification({ title: "Project Legacy", body: String(body || "") });
    },

    // background slideshow / live video
    listBackgrounds: () => invoke("list_backgrounds"),
    listLive: () => invoke("list_live"),

    // links
    openLink: (url) => invoke("open_link", { url }),
    openOfficialSite: () => invoke("open_official_site"),

    // install (download + extract)
    getState: () => invoke("get_state"),
    installStart: () => invoke("install_start"),
    installPause: () => invoke("install_pause"),
    installCancel: () => invoke("install_cancel"),

    // game
    launchGame: () => invoke("game_launch"),
    stopGame: () => invoke("game_stop"),

    // The bundled original Injector.exe handles the Redirector and unlocked
    // Console modules. Legacy anti-cheat process stubs are not started.
    injectModule: (key) => invoke("inject_module", { key }),
    consoleStatus: () => invoke("console_status"),
    consoleUnlock: (code) => invoke("console_unlock", { code }),
    consoleLock: () => invoke("console_lock"),

    // live events
    onMatchEvent: subscribe("match-event"),
    onClientsUpdate: subscribe("clients-update"),
    onNetworkStatus: subscribe("network-status"),
    onHealthStatus: subscribe("health-status"),
    onNews: subscribe("news-update"),
    onDiscordCount: subscribe("discord-count"),
    onProfile: subscribe("profile-update"),
    onUpdate: subscribe("update-status"),
    onInstallProgress: subscribe("install-progress"),
    onGameStatus: subscribe("game-status"),

    checkLauncherUpdate: () => invoke("check_launcher_update"),
    installLauncherUpdate: () => invoke("install_launcher_update"),
    onLauncherUpdateProgress: subscribe("launcher-update-progress"),
    preflight: () => invoke("preflight"),
    folderInfo: () => invoke("folder_info"),
    getShop: () => invoke("get_shop"),
    setLang: (lang) => invoke("set_lang", { lang })
  };
})();
