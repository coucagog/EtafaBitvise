@echo off
rem Lance le build de debogage de MLS Connect depuis une copie placee hors du dossier du projet.
rem Raison : le dossier du projet porte une etiquette d'integrite faible (bac a sable de l'agent).
rem Un programme lance depuis ce dossier tourne en integrite faible : il ne peut ecrire ni dans
rem %TEMP% ni dans le cache WebView2, et la fenetre reste blanche (0x800700AA ou 0x8000FFFF).
rem Parametre facultatif : arguments supplementaires pour WebView2.
setlocal
set "SRC=%~dp0..\target\debug\mls-connect.exe"
set "DST=%LOCALAPPDATA%\mls-connect-essai"
if not exist "%SRC%" (echo Executable absent : %SRC% & exit /b 1)
if not exist "%DST%" mkdir "%DST%"
copy /y "%SRC%" "%DST%\mls-connect.exe" >nul || (echo Copie impossible : MLS Connect est-il encore ouvert ? & exit /b 1)
if not "%~1"=="" set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=%~1"
start "" "%DST%\mls-connect.exe"
