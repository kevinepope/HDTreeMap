@echo off
rem run-windows.cmd: double-click in Explorer (or run from a terminal) to build and launch
rem HDTreeMap on Windows. It builds an optimised release binary the first time, which takes
rem a few minutes; later launches only rebuild what changed. Arguments are passed through to
rem the app, e.g. `run-windows.cmd C:\Users\me\Downloads` or `run-windows.cmd --demo laptop-ssd`.
rem The Windows counterpart of run-mac.command.
rem
rem The build expects the agg-gui checkout beside this repo (..\agg-gui), as in the
rem rust-apps superproject; see README "Build from source".

setlocal
cd /d "%~dp0" || exit /b 1

rem A shell opened before rustup was installed may not have cargo on PATH yet.
where cargo >nul 2>nul
if errorlevel 1 if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where cargo >nul 2>nul || goto no_cargo
if not exist "..\agg-gui\" goto no_agg_gui

echo Building HDTreeMap (release)...
cargo build --release -p hdtreemap-native || goto build_failed

echo Starting HDTreeMap...
"target\release\hdtreemap.exe" %*
exit /b %errorlevel%

:no_cargo
set "HDTREEMAP_FAIL=cargo was not found. Install Rust from https://rustup.rs and try again."
goto fail

:no_agg_gui
set "HDTREEMAP_FAIL=..\agg-gui was not found. Clone https://github.com/larsbrubaker/agg-gui beside this folder (or use the rust-apps superproject)."
goto fail

:build_failed
set "HDTREEMAP_FAIL=the build failed (see the errors above). If it asks for a newer rustc, run: rustup update"
goto fail

:fail
echo.
echo HDTreeMap: %HDTREEMAP_FAIL%
rem Keep the console window open so the message can be read after a double-click.
pause
exit /b 1
