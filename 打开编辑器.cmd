@echo off
setlocal
chcp 65001 >nul
set "OPENGALGAME_EDITOR=%~dp0release\opengalgame.exe"
if not exist "%OPENGALGAME_EDITOR%" goto from_source
if /i "%~1"=="--check" (
  echo PASS: portable editor and source project are available.
  if not exist "%~dp0examples\long-form\project.json" exit /b 1
  exit /b 0
)
start "" /D "%~dp0" "%OPENGALGAME_EDITOR%" --hub
exit /b 0
:from_source
if /i "%~1"=="--check" (
  echo Portable editor not bundled. Use the source build launcher.
  exit /b 1
)
call "%~dp0从源码编译运行.cmd"
exit /b %errorlevel%
