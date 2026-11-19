import { createIcons, icons } from "lucide";

// Expose Lucide for the existing vanilla-JavaScript launcher without changing
// the Tauri bridge or introducing a runtime CDN dependency.
window.ProjectLegacyIcons = {
  createIcons: () => createIcons({ icons }),
  icons,
};

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => window.ProjectLegacyIcons.createIcons(), { once: true });
} else {
  window.ProjectLegacyIcons.createIcons();
}

// Convert Lucide placeholders added later by existing launcher views as well.
const observer = new MutationObserver(() => window.ProjectLegacyIcons.createIcons());
observer.observe(document.documentElement, { childList: true, subtree: true });
