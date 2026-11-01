PROJECT LEGACY LAUNCHER 3.0

INSTALLATION (Windows)
  1. Installe Node.js (LTS).
  2. Ouvre un terminal dans ce dossier.
  3. npm install
  4. npm start

CREER UN .EXE PORTABLE (optionnel)
  npm run dist      -> le .exe apparait dans le dossier "dist"

UTILISATION
  1. Settings > Installation folder > CHOOSE : dossier ou installer le jeu (beaucoup d'espace libre).
  2. Settings > Account : email + mot de passe du compte Project Legacy > SAVE.
  3. Dashboard > DOWNLOAD BUILD 4.5 : telechargement (pause/reprise possibles) puis extraction automatique.
  4. Dashboard > PLAY : lance le jeu (comme le launcher officiel) et injecte projectlegacy.dll (redirector).
  5. Une fois dans le lobby : bouton INJECT CONSOLE, puis touche F3 en jeu.
     INJECT ANTI-CRASH seulement si le jeu plante (necessite anticrash.dll dans bin/).

  Si tu as deja le build extrait, choisis simplement son dossier : le launcher le detecte (PLAY apparait).

DOSSIER bin/
  Injector.exe, projectlegacy.dll, edit.dll doivent rester dans bin/.
  anticrash.dll (optionnel) : copie-le aussi dans bin/ pour activer le bouton INJECT ANTI-CRASH.
  SHA-256 des fichiers fournis :
    Injector.exe       f733438319af6f3c7dc40b3057b1c9239fc691b0b808311ba3e5b699f22157e4
    projectlegacy.dll  94f9822a72d169f4c6a6303ecabf9f68fee0a9dac24c8dacc29161760a8b9905
    edit.dll           b0dfce0ec0f866e4881d55796f234cc2119e6e82e6728d1835c164f3da3ef6d2
  Windows Defender peut mettre Injector.exe en quarantaine (c'est un injecteur de DLL).
  Bouton Settings > Launcher files > VERIFY pour voir si tout est present.

REGLAGES A CONNAITRE (main.js, en haut du fichier)
  INJECTIONS : delai (en ms apres le lancement du jeu) avant d'injecter le redirector.
  Si le jeu plante au demarrage, c'est le premier chiffre a changer.
  Arguments du jeu : Settings > Launch options.

NOTES
  - assets/background.jpg : image de fond du dashboard (optionnelle, un degrade violet sinon).
  - Si l'extraction automatique echoue, extrais 4.5.rar a la main (WinRAR/7-Zip) dans le dossier choisi.
  - L'archive est supprimee apres une installation reussie.


RELEASE VIA GITHUB ACTIONS (depot legacyfn/launcher)
  1. Sur GitHub, ouvre package.json et monte "version" (ex. 4.2.1), puis Commit.
  2. Onglet Actions > "Build launcher" > Run workflow (laisse "Publiceren" coche) > Run.
  3. Apres quelques minutes, la release apparait dans Releases avec :
       - Project Legacy Launcher Setup X.Y.Z.exe
       - Project Legacy Launcher Setup X.Y.Z.exe.blockmap
       - latest.yml
  4. Les launchers deja installes (version installee, pas le .exe portable) verifient les releases
     au demarrage puis toutes les 4 h, telechargent la mise a jour et proposent "RESTART & UPDATE".
  Notes :
  - Le depot doit etre public. Sans nouvelle version dans package.json, aucune mise a jour n'est vue.
  - Pour tester sans publier : Run workflow en decochant "Publiceren" (le .exe est dans les artifacts).
  - La premiere installation (4.x) se fait a la main avec le Setup .exe.


TRADUCTIONS (en / fr / nl)
  Le texte du code est en anglais. Tout texte affiche (boutons, messages, erreurs, fenetres de
  confirmation, placeholders) est traduit a l'affichage via le bloc I18N_ROWS / I18N_PATTERNS
  dans index.html (cherche "I18N_ROWS"). Un texte sans entree reste simplement en anglais.
  - Texte fixe : ajoute une ligne  ["English", "Francais", "Nederlands"]
  - Message avec variables (nombres, noms) : ajoute un motif  [/^regex$/, "fr", "nl"]
    ($1 = texte capture tel quel, {1} = texte capture et traduit aussi)
  - Si tu changes un message anglais dans main.js, change aussi sa cle dans I18N_ROWS.
  - Nouvelle langue : ajoute une colonne a chaque ligne/motif + une <option> dans #langSelect.
  Les logs (COPY LOGS) restent en anglais, pour le support.
