@echo off
REM Windows: double-click to get a link you can send to someone.
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "demo\share.ps1"
