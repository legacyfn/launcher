
make this english
PROJECT LEGACY LAUNCHER 3.0

INSTALLATION (Windows)

Install Node.js (LTS).

Open a terminal in this folder.

npm install

npm start

CREATE A PORTABLE .EXE (optional)
npm run dist -> the .exe appears in the "dist" folder

USAGE

Settings > Installation folder > CHOOSE: folder where the game will be installed (lots of free space).

Settings > Account: email + password of the Project Legacy account > SAVE.

Dashboard > DOWNLOAD BUILD 4.5: download (pause/resume possible) then automatic extraction.

Dashboard > PLAY: launches the game (like the official launcher) and injects projectlegacy.dll (redirector).

Once in the lobby: INJECT CONSOLE button, then press F3 in game.
INJECT ANTI-CRASH only if the game crashes (requires anticrash.dll in bin/).

If you already have the build extracted, simply choose its folder: the launcher detects it (PLAY appears).

bin/ FOLDER
Injector.exe, projectlegacy.dll, edit.dll must stay in bin/.
anticrash.dll (optional): also copy it into bin/ to enable the INJECT ANTI-CRASH button.
SHA-256 of the provided files:
Injector.exe f733438319af6f3c7dc40b3057b1c9239fc691b0b808311ba3e5b699f22157e4
projectlegacy.dll 94f9822a72d169f4c6a6303ecabf9f68fee0a9dac24c8dacc29161760a8b9905
edit.dll b0dfce0ec0f866e4881d55796f234cc2119e6e82e6728d1835c164f3da3ef6d2
Windows Defender may quarantine Injector.exe (it is a DLL injector).
Settings > Launcher files > VERIFY button to check that everything is present.

SETTINGS TO KNOW (main.js, at the top of the file)
INJECTIONS: delay (in ms after the game launches) before injecting the redirector.
If the game crashes on startup, this is the first number to change.
Game arguments: Settings > Launch options.

NOTES

assets/background.jpg: dashboard background image (optional, a purple gradient otherwise).

If automatic extraction fails, extract 4.5.rar manually (WinRAR/7-Zip) into the chosen folder.

The archive is deleted after a successful installation.

RELEASE VIA GITHUB ACTIONS (repository legacyfn/launcher)

On GitHub, open package.json and bump "version" (e.g. 4.2.1), then Commit.

Actions tab > "Build launcher" > Run workflow (leave "Publiceren" checked) > Run.

After a few minutes, the release appears in Releases with:

Project Legacy Launcher Setup X.Y.Z.exe

Project Legacy Launcher Setup X.Y.Z.exe.blockmap

latest.yml

Already installed launchers (installed version, not the portable .exe) check the releases
on startup and then every 4 h, download the update and offer "RESTART & UPDATE".
Notes:

The repository must be public. Without a new version in package.json, no update is seen.

To test without publishing: Run workflow with "Publiceren" unchecked (the .exe is in the artifacts).

The first installation (4.x) is done manually with the Setup .exe.

TRANSLATIONS (en / fr / nl)
The code text is in English. All displayed text (buttons, messages, errors, confirmation
windows, placeholders) is translated at display time via the I18N_ROWS / I18N_PATTERNS block
in index.html (search for "I18N_ROWS"). Text without an entry simply stays in English.

Fixed text: add a line ["English", "Francais", "Nederlands"]

Message with variables (numbers, names): add a pattern [/^regex$/, "fr", "nl"]
($1 = captured text as-is, {1} = captured text also translated)

If you change an English message in main.js, also change its key in I18N_ROWS.

New language: add a column to each line/pattern + an <option> in #langSelect.
Logs (COPY LOGS) remain in English, for support.
