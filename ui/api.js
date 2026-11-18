// Drop-in replacement for the Electron preload.js. index.html calls
// `launcher.xxx(...)` exactly like before; this file just routes those
// calls to Tauri commands/events instead of ipcRenderer. No other
// frontend file needed to change.
(function () {
  "use strict";
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
