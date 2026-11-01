"use strict";

// Default game arguments (editable in Settings). Sign-in arguments are added separately.
// Same arguments as the official Project Legacy launcher. -HTTP=WinINet matters: the
// redirector DLL hooks WinINet, so the game must use it for its web requests.
const DEFAULT_ARGS = "-epicapp=Fortnite -skippatchcheck -epicportal -HTTP=WinINet -log";

/** Split a command-line string into arguments, honouring "double quotes". */
function splitArgs(text) {
  const out = [];
  const re = /"([^"]*)"|(\S+)/g;
  let m;
  while ((m = re.exec(String(text || "")))) out.push(m[1] !== undefined ? m[1] : m[2]);
  return out;
}

/** Final argument list handed to the game executable (never goes through a shell). */
function buildLaunchArgs({ extraArgs, email, password }) {
  const args = splitArgs(extraArgs);
  // The launcher owns the sign-in arguments, so drop any copy typed in the free-text field.
  const filtered = args.filter(a => !/^-AUTH_(LOGIN|PASSWORD|TYPE)=/i.test(a));
  filtered.push(`-AUTH_LOGIN=${email}`, `-AUTH_PASSWORD=${password}`, "-AUTH_TYPE=epic");
  return filtered;
}

module.exports = { DEFAULT_ARGS, splitArgs, buildLaunchArgs };
