import { createIcons, icons } from "lucide";

const iconForPage = {
  home: "house",
  community: "users",
  profile: "user-round",
  shop: "shopping-bag",
  downloads: "download",
  settings: "settings",
};

function enhanceLauncherUi() {
  // Change only the contents of icon slots; never replace navigation buttons
  // or touch their listeners, attributes, or click targets.
  document.querySelectorAll(".nav button[data-page]").forEach((button) => {
    const slot = button.querySelector(".nav-icon");
    const name = iconForPage[button.dataset.page];
    if (!slot || !name || slot.querySelector("svg")) return;

    const icon = document.createElement("i");
    icon.setAttribute("data-lucide", name);
    icon.setAttribute("aria-hidden", "true");
    slot.replaceChildren(icon);
  });

  // Keep DaisyUI styling opt-in and scoped to existing non-navigation controls.
  document.querySelectorAll("button.small-button").forEach((el) => {
    el.classList.add("btn", "btn-sm", "btn-outline");
  });
  document.querySelectorAll("button.button").forEach((el) => el.classList.add("btn"));
  document.querySelectorAll("button.button.primary, button.primary").forEach((el) => {
    el.classList.add("btn-primary");
  });
  document.querySelectorAll("button.button.secondary, button.secondary").forEach((el) => {
    el.classList.add("btn-outline");
  });
  document.querySelectorAll("input.field, select.field, textarea.field").forEach((el) => {
    el.classList.add(el.tagName === "SELECT" ? "select" : "input", "input-bordered", "input-sm");
  });
  document.querySelectorAll("select:not(.field)").forEach((el) => {
    el.classList.add("select", "select-bordered", "select-sm");
  });
  document.querySelectorAll("input[type='checkbox']").forEach((el) => {
    el.classList.add("checkbox", "checkbox-primary", "checkbox-sm");
  });

  document.querySelectorAll(".stat-icon").forEach((slot) => {
    if (slot.querySelector("svg")) return;
    const old = slot.textContent.trim();
    const name = old === "◉" ? "users" : old === "✦" ? "sparkles" : old === "⌁" ? "activity" : null;
    if (!name) return;

    const icon = document.createElement("i");
    icon.setAttribute("data-lucide", name);
    icon.setAttribute("aria-hidden", "true");
    slot.replaceChildren(icon);
  });

  createIcons({ icons, attrs: { "stroke-width": 1.8 } });
}

window.ProjectLegacyIcons = { createIcons: enhanceLauncherUi, icons };

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", enhanceLauncherUi, { once: true });
} else {
  enhanceLauncherUi();
}
