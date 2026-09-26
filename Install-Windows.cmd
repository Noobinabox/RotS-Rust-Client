@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
set "installerExit=%ERRORLEVEL%"
echo.
if not "%installerExit%"=="0" echo Installation did not complete. Review the error above before retrying.
pause
exit /b %installerExit%
