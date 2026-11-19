import { createIcons, icons } from "lucide";

const iconForPage = {
  home: "house", community: "users", profile: "user-round", shop: "shopping-bag",
  downloads: "download", settings: "settings",
};

function enhanceLauncherUi() {
  // Replace the old text glyphs in the sidebar with consistent Lucide icons.
  document.querySelectorAll(".nav button[data-page]").forEach((button) => {
    const slot = button.querySelector(".nav-icon");
    const name = iconForPage[button.dataset.page];
    if (!slot || !name || slot.querySelector("svg")) return;
    slot.replaceChildren();
    const icon = document.createElement("i");
    icon.setAttribute("data-lucide", name);
    icon.setAttribute("aria-hidden", "true");
    slot.append(icon);
  });

  // Give existing controls DaisyUI's component classes while retaining their
  // IDs, event listeners, and Project Legacy-specific layout classes.
  document.querySelectorAll("button.small-button").forEach((el) => el.classList.add("btn", "btn-sm", "btn-outline"));
  document.querySelectorAll("button.button").forEach((el) => el.classList.add("btn"));
  document.querySelectorAll("button.button.primary, button.primary").forEach((el) => el.classList.add("btn-primary"));
  document.querySelectorAll("button.button.secondary, button.secondary").forEach((el) => el.classList.add("btn-outline"));
  document.querySelectorAll("input.field, select.field, textarea.field").forEach((el) => el.classList.add(el.tagName === "SELECT" ? "select" : "input", "input-bordered", "input-sm"));
  document.querySelectorAll("select:not(.field)").forEach((el) => el.classList.add("select", "select-bordered", "select-sm"));
  document.querySelectorAll("input[type='checkbox']").forEach((el) => el.classList.add("checkbox", "checkbox-primary", "checkbox-sm"));

  // A few existing statistic glyphs receive semantic icons based on their text.
  document.querySelectorAll(".stat-icon").forEach((slot) => {
    if (slot.querySelector("svg")) return;
    const old = slot.textContent.trim();
    const name = old === "◉" ? "users" : old === "✦" ? "sparkles" : old === "⌁" ? "activity" : null;
    if (!name) return;
    slot.replaceChildren();
    const icon = document.createElement("i");
    icon.setAttribute("data-lucide", name);
    icon.setAttribute("aria-hidden", "true");
    slot.append(icon);
  });

  createIcons({ icons, attrs: { "stroke-width": 1.8 } });
}

window.ProjectLegacyIcons = { createIcons: enhanceLauncherUi, icons };

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", enhanceLauncherUi, { once: true });
} else {
  enhanceLauncherUi();
}

// Refresh icons after existing launcher views create new elements dynamically.
let refreshQueued = false;
const observer = new MutationObserver(() => {
  if (refreshQueued) return;
  refreshQueued = true;
  queueMicrotask(() => {
    refreshQueued = false;
    enhanceLauncherUi();
  });
});
observer.observe(document.documentElement, { childList: true, subtree: true });
