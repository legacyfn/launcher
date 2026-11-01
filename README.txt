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


MISE A JOUR AUTOMATIQUE DU LAUNCHER (v4.1+)
  1. Cree un depot GitHub (public) et mets ton pseudo + le nom du depot dans package.json (section "publish").
  2. Pour sortir une version : monte "version" dans package.json (ex. 4.2.0), lance build.bat.
  3. Sur GitHub : Releases > Draft a new release > tag v4.2.0 > ajoute CES 3 fichiers du dossier dist :
       - Project Legacy Launcher Setup 4.2.0.exe
       - Project Legacy Launcher Setup 4.2.0.exe.blockmap
       - latest.yml
     puis Publish release.
  4. Les launchers deja installes (version installee, pas le .exe portable) telechargent la mise a jour
     et proposent "RESTART & UPDATE". La premiere installation 4.1.0 se fait a la main (Setup .exe).
