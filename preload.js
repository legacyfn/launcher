const { contextBridge, ipcRenderer } = require("electron");

const subscribe = channel => callback => {
  const listener = (_event, data) => callback(data);
  ipcRenderer.on(channel, listener);
  return () => ipcRenderer.removeListener(channel, listener);
};

contextBridge.exposeInMainWorld("launcher", {
  // configuration
  getConfig: () => ipcRenderer.invoke("get-config"),
  chooseFolder: () => ipcRenderer.invoke("choose-folder"),
  checkFolder: () => ipcRenderer.invoke("check-folder"),
  openFolder: () => ipcRenderer.invoke("open-folder"),
  saveAccount: data => ipcRenderer.invoke("save-account", data),
  saveLaunchOptions: data => ipcRenderer.invoke("save-launch-options", data),
  verifyFiles: () => ipcRenderer.invoke("verify-files"),

  // background slideshow
  listBackgrounds: () => ipcRenderer.invoke("list-backgrounds"),

  // links
  openLink: url => ipcRenderer.invoke("open-link", url),
  openOfficialSite: () => ipcRenderer.invoke("open-official-site"),

  // install (download + extract)
  getState: () => ipcRenderer.invoke("get-state"),
  installStart: () => ipcRenderer.invoke("install-start"),
  installPause: () => ipcRenderer.invoke("install-pause"),
  installCancel: () => ipcRenderer.invoke("install-cancel"),

  // game
  launchGame: () => ipcRenderer.invoke("game-launch"),
  stopGame: () => ipcRenderer.invoke("game-stop"),
  injectModule: key => ipcRenderer.invoke("game-inject", key),

  // live events
  onMatchEvent: subscribe("match-event"),
  onClientsUpdate: subscribe("clients-update"),
  onNetworkStatus: subscribe("network-status"),
  onHealthStatus: subscribe("health-status"),
  onNews: subscribe("news-update"),
  onDiscordCount: subscribe("discord-count"),
  onProfile: subscribe("profile-update"),
  onUpdate: subscribe("update-status"),
  installUpdate: () => ipcRenderer.invoke("update-install"),
  runDiagnostics: fix => ipcRenderer.invoke("run-diagnostics", fix),
  resetGameSettings: () => ipcRenderer.invoke("reset-game-settings"),
  getCompat: () => ipcRenderer.invoke("get-compat"),
  setCompat: on => ipcRenderer.invoke("set-compat", on),
  preflight: () => ipcRenderer.invoke("preflight"),
  getLogs: () => ipcRenderer.invoke("get-logs"),
  onInstallProgress: subscribe("install-progress"),
  onGameStatus: subscribe("game-status")
});
