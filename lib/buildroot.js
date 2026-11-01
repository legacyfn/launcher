"use strict";
const fs = require("fs");
const path = require("path");

const GAME_EXE_REL = path.join("FortniteGame", "Binaries", "Win64", "FortniteClient-Win64-Shipping.exe");

function isDir(p) {
  try { return fs.statSync(p).isDirectory(); } catch { return false; }
}

/**
 * Find the folder that contains FortniteGame/Binaries/Win64/FortniteClient-Win64-Shipping.exe.
 * Looks in `dir` itself and up to `maxDepth` levels of sub-folders (archives usually
 * extract into one top-level folder). Returns the absolute build root, or null.
 */
function findBuildRoot(dir, maxDepth = 2) {
  if (!dir || !isDir(dir)) return null;
  if (fs.existsSync(path.join(dir, GAME_EXE_REL))) return dir;
  if (maxDepth <= 0) return null;

  let entries;
  try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return null; }
  for (const entry of entries) {
    if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
    const found = findBuildRoot(path.join(dir, entry.name), maxDepth - 1);
    if (found) return found;
  }
  return null;
}

module.exports = { findBuildRoot, GAME_EXE_REL };
