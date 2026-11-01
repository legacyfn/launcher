"use strict";
// Runs in a worker thread so extracting multi-GB archives never freezes the launcher window.
const { parentPort, workerData } = require("worker_threads");
const fs = require("fs");
const path = require("path");

const SAFETY_MARGIN = 256 * 1024 * 1024;

const post = msg => parentPort.postMessage(msg);
const gb = n => (n / 1024 ** 3).toFixed(1) + " GB";

function isSafeName(name) {
  const n = String(name).replace(/\\/g, "/");
  return !(n.startsWith("/") || /^[a-zA-Z]:/.test(n) || n.split("/").includes(".."));
}

async function loadCreateExtractor() {
  let mod;
  try {
    mod = await import("node-unrar-js");
  } catch (err) {
    throw new Error("The RAR extraction module is missing. Run \"npm install\" in the launcher folder.");
  }
  const fn = mod.createExtractorFromFile || (mod.default && mod.default.createExtractorFromFile);
  if (!fn) throw new Error("Unsupported node-unrar-js version (createExtractorFromFile not found).");
  return fn;
}

function describe(err) {
  if (err && err.reason) {
    return `${err.reason}${err.file ? ` on ${err.file}` : ""}. The archive may be corrupt: use "Reinstall" to download it again.`;
  }
  return (err && err.message) || String(err);
}

async function main() {
  const { archive, target } = workerData;
  fs.mkdirSync(target, { recursive: true });
  const createExtractor = await loadCreateExtractor();

  // Pass 1: read the headers only (fast) to validate the archive and size the job.
  const lister = await createExtractor({ filepath: archive, targetPath: target });
  const headers = [...lister.getFileList().fileHeaders];
  const files = headers.filter(h => !h.flags.directory);
  if (!files.length) throw new Error("The archive is empty.");

  for (const h of headers) {
    if (!isSafeName(h.name)) throw new Error(`Refusing to extract unsafe path: ${h.name}`);
    if (h.flags.encrypted) throw new Error("The archive is password protected.");
  }

  const bytesTotal = files.reduce((sum, h) => sum + (Number(h.unpSize) || 0), 0);
  try {
    const s = fs.statfsSync(target);
    const free = Number(s.bavail) * Number(s.bsize);
    if (free < bytesTotal + SAFETY_MARGIN) {
      throw new Error(`Not enough disk space: ${gb(bytesTotal)} needed, ${gb(free)} free.`);
    }
  } catch (err) {
    if (/Not enough disk space/.test(err.message)) throw err; // statfs itself failing is not fatal
  }

  post({ type: "progress", done: 0, total: files.length, bytesDone: 0, bytesTotal, name: "" });

  // Pass 2: extract. Each iteration of the generator extracts one entry to disk.
  const extractor = await createExtractor({ filepath: archive, targetPath: target });
  const { files: iterator } = extractor.extract();
  let done = 0;
  let bytesDone = 0;
  let lastPost = 0;
  for (const item of iterator) {
    const h = item.fileHeader;
    if (h.flags.directory) continue;
    done++;
    bytesDone += Number(h.unpSize) || 0;
    const now = Date.now();
    if (now - lastPost > 150 || done === files.length) {
      lastPost = now;
      post({ type: "progress", done, total: files.length, bytesDone, bytesTotal, name: h.name });
    }
  }
  post({ type: "done" });
}

main().catch(err => post({ type: "error", message: describe(err) }));
