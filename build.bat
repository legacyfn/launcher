@echo off
cd /d "%~dp0"
where node >nul 2>nul
if errorlevel 1 (
  echo Node.js n est pas installe. Installe la version LTS depuis https://nodejs.org puis relance ce fichier.
  pause
  exit /b 1
)
call npm install
if errorlevel 1 (pause & exit /b 1)
call npm run dist
if errorlevel 1 (pause & exit /b 1)
echo.
echo Termine ! Ton launcher .exe est dans le dossier "dist".
explorer dist
pause
