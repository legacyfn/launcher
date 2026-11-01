"use strict";
// Resumable HTTP(S) downloader (Range requests, redirects, automatic retries).
// Pure Node, no Electron dependency, so it can be tested on its own.

const fs = require("fs");
const http = require("http");
const https = require("https");

class Aborted extends Error {
  constructor(reason) {
    super(`Download aborted (${reason})`);
    this.aborted = reason;
  }
}

const sleep = ms => new Promise(r => setTimeout(r, ms));
const REDIRECTS = new Set([301, 302, 303, 307, 308]);

function fatal(message) {
  const err = new Error(message);
  err.fatal = true;
  return err;
}

/**
 * Start (or resume) a download into `partPath`.
 * Returns { promise, abort(reason) }. The promise resolves with { size, total }.
 * When aborted it rejects with an Aborted error (err.aborted === reason),
 * and the partial file is kept so the download can be resumed later.
 */
function download({
  url,
  partPath,
  userAgent = "Mozilla/5.0",
  onProgress = () => {},
  maxRetries = 5,
  retryDelayMs = 3000,
  timeoutMs = 30000,
  tickMs = 250
}) {
  let current = null;
  let abortReason = null;

  function attempt() {
    return new Promise((resolve, reject) => {
      let settled = false;
      let ticker = null;
      const finish = (fn, value) => {
        if (settled) return;
        settled = true;
        clearInterval(ticker);
        fn(value);
      };

      const open = (target, hops) => {
        let offset = 0;
        try { offset = fs.statSync(partPath).size; } catch {}

        const lib = target.startsWith("https:") ? https : http;
        const headers = { "User-Agent": userAgent, "Accept-Encoding": "identity" };
        if (offset > 0) headers.Range = `bytes=${offset}-`;

        const req = lib.get(target, { headers }, res => {
          const code = res.statusCode;

          if (REDIRECTS.has(code) && res.headers.location) {
            res.resume();
            if (hops >= 6) return finish(reject, fatal("Too many redirects"));
            return open(new URL(res.headers.location, target).toString(), hops + 1);
          }

          if (code === 416) {
            // Range not satisfiable: the partial file may already be the whole file.
            res.resume();
            const m = /\/(\d+)$/.exec(res.headers["content-range"] || "");
            if (m && Number(m[1]) === offset) return finish(resolve, { size: offset, total: offset });
            try { fs.rmSync(partPath, { force: true }); } catch {}
            return finish(reject, new Error("Server rejected the resume request, restarting from zero"));
          }

          if (code !== 200 && code !== 206) {
            res.resume();
            const msg = `HTTP ${code}`;
            const retryable = code >= 500 || code === 408 || code === 429;
            return finish(reject, retryable ? new Error(msg) : fatal(msg));
          }

          let start = offset;
          let total = 0;
          if (code === 206) {
            const m = /bytes (\d+)-\d+\/(\d+|\*)/.exec(res.headers["content-range"] || "");
            if (m) start = Number(m[1]);
            if (m && m[2] !== "*") total = Number(m[2]);
            else if (res.headers["content-length"]) total = start + Number(res.headers["content-length"]);
          } else {
            // 200: the server ignored Range, so we start over from byte 0.
            start = 0;
            total = Number(res.headers["content-length"] || 0);
          }

          const out = fs.createWriteStream(partPath, { flags: start > 0 ? "a" : "w" });
          let written = 0;
          let lastBytes = 0;
          let lastTime = Date.now();
          let speed = 0;
          let ended = false;

          const emit = () => {
            const now = Date.now();
            const dt = Math.max(1, now - lastTime) / 1000;
            const inst = (written - lastBytes) / dt;
            speed = speed ? speed * 0.7 + inst * 0.3 : inst;
            lastBytes = written;
            lastTime = now;
            const received = start + written;
            onProgress({
              received,
              total,
              speed,
              eta: total && speed > 1 ? (total - received) / speed : null
            });
          };
          ticker = setInterval(emit, tickMs);
          emit();

          res.on("data", chunk => { written += chunk.length; });
          res.on("error", err => finish(reject, err));
          res.on("end", () => { ended = true; });
          res.on("close", () => {
            if (!ended) finish(reject, new Error("Connection closed before the download finished"));
          });

          out.on("error", err => {
            const e = new Error(`Cannot write to disk: ${err.message}`);
            if (["ENOSPC", "EACCES", "EPERM", "EROFS", "EISDIR"].includes(err.code)) e.fatal = true;
            res.destroy();
            finish(reject, e);
          });
          out.on("finish", () => {
            const size = start + written;
            emit();
            if (total && size !== total) {
              return finish(reject, new Error(`Incomplete download (${size} of ${total} bytes)`));
            }
            finish(resolve, { size, total: total || size });
          });

          res.pipe(out);
        });

        current = req;
        req.setTimeout(timeoutMs, () => req.destroy(new Error("Connection timed out")));
        req.on("error", err => finish(reject, err));
      };

      open(url, 0);
    });
  }

  const promise = (async () => {
    let tries = 0;
    for (;;) {
      if (abortReason) throw new Aborted(abortReason);
      try {
        return await attempt();
      } catch (err) {
        if (abortReason) throw new Aborted(abortReason);
        if (err.fatal || ++tries > maxRetries) throw err;
        onProgress({ retrying: tries, error: err.message });
        await sleep(retryDelayMs * tries);
      }
    }
  })();

  return {
    promise,
    abort(reason = "abort") {
      abortReason = reason;
      if (current) current.destroy();
    }
  };
}

module.exports = { download, Aborted };
