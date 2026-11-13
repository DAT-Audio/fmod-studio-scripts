@echo off
REM ============================================================
REM  FMOD Event Recorder - one-click setup (Windows)
REM  Version: 0.6.5 (DAT-audio Utilities pack)
REM
REM  Double-click this file, or run it from a terminal.
REM  It finds Python 3, creates a local environment with the
REM  libraries the tool needs, and verifies everything loads.
REM  Needs the internet once (to download the libraries).
REM  Python is the only thing you install yourself.
REM ============================================================
setlocal enabledelayedexpansion
cd /d "%~dp0"
set "SELF=%CD%"
set "VENV=%SELF%\venv"
set "REQ=%SELF%\requirements.txt"
set "LIBS=%SELF%\libs\windows"
set "PYFMODEX_DLL_PATH=%LIBS%\fmod.dll"
set "PYFMODEX_STUDIO_DLL_PATH=%LIBS%\fmodstudio.dll"

echo.
echo  ===============================================
echo      FMOD Event Recorder - Setup (Windows)
echo      Event Recorder v0.6.5
echo  ===============================================
echo.

REM --- 1. Find Python 3 -------------------------------------
set "PY="
py -3 --version >nul 2>&1 && set "PY=py -3"
if not defined PY ( python --version >nul 2>&1 && set "PY=python" )
if not defined PY (
    echo Python 3 was not found.
    echo Install it from https://www.python.org/downloads/  (tick "Add Python to PATH"^), then run this again.
    pause & exit /b 1
)
for /f "delims=" %%v in ('%PY% -c "import sys;print('%%d.%%d.%%d'%%sys.version_info[:3])"') do set "PYVER=%%v"
%PY% -c "import sys;raise SystemExit(0 if sys.version_info[:2]>=(3,9) else 1)"
if errorlevel 1 (
    echo Python 3.9 or newer is required ^(found %PYVER%^).
    pause & exit /b 1
)
echo Found Python %PYVER%.

REM --- 1b. Check the FMOD libraries are in place -------------
REM  They aren't shipped with the tool (FMOD doesn't allow tool makers to
REM  share them), so each user adds their own before running setup. Accept the
REM  flat layout or an x64 / arm64 subfolder, like the renderer does.
set "LIBDIR="
if exist "%LIBS%\fmod.dll" if exist "%LIBS%\fmodstudio.dll" set "LIBDIR=%LIBS%"
if not defined LIBDIR if exist "%LIBS%\x64\fmod.dll" if exist "%LIBS%\x64\fmodstudio.dll" set "LIBDIR=%LIBS%\x64"
if not defined LIBDIR if exist "%LIBS%\arm64\fmod.dll" if exist "%LIBS%\arm64\fmodstudio.dll" set "LIBDIR=%LIBS%\arm64"
if defined LIBDIR goto libs_ok
echo.
echo The FMOD Engine libraries aren't in place yet.
echo   They aren't included with this tool: FMOD's position is that tool and
echo   script makers must not publish or share them, so you add your own copy.
echo.
echo   1. Sign in at https://www.fmod.com/download - free - and download the
echo      FMOD Engine for Windows - 2.03.x recommended.
echo   2. From the download, copy these two files:
echo        api\core\lib\x64\fmod.dll
echo        api\studio\lib\x64\fmodstudio.dll
echo      into:
echo        %LIBS%\
echo   3. Run this setup again.
echo.
echo   Details: libs\windows\PUT_WINDOWS_DLLS_HERE.txt, or README.md - Setup, step 2.
pause
exit /b 1
:libs_ok
set "PYFMODEX_DLL_PATH=%LIBDIR%\fmod.dll"
set "PYFMODEX_STUDIO_DLL_PATH=%LIBDIR%\fmodstudio.dll"
echo FMOD libraries found in %LIBDIR%.

REM --- 2. Already set up? -----------------------------------
if exist "%VENV%\Scripts\python.exe" (
    "%VENV%\Scripts\python.exe" -c "from pyfmodex.studio import StudioSystem; import numpy, soundfile" >nul 2>&1
    if not errorlevel 1 (
        echo Looks like you're already set up.
        set /p ANS="Reinstall from scratch anyway? [y/N] "
        if /i not "!ANS!"=="y" ( echo OK, leaving as-is. & goto done )
    )
)

REM --- 3. Create the virtual environment --------------------
if exist "%VENV%" rmdir /s /q "%VENV%"
echo Setting up the local Python environment...
%PY% -m venv "%VENV%"
if errorlevel 1 ( echo Failed to create the virtual environment. & pause & exit /b 1 )

REM --- 4. Install dependencies ------------------------------
echo Installing libraries (this can take a minute)...
"%VENV%\Scripts\python.exe" -m pip install --upgrade pip >nul 2>&1
"%VENV%\Scripts\python.exe" -m pip install -r "%REQ%"
if errorlevel 1 ( echo Library installation failed. Check your internet connection and try again. & pause & exit /b 1 )

REM --- 5. Verify -------------------------------------------
echo Checking everything loads...
"%VENV%\Scripts\python.exe" -c "import os; from pyfmodex.studio import StudioSystem; import numpy, soundfile; print('  pyfmodex OK  |  numpy '+numpy.__version__+'  |  soundfile '+soundfile.__version__)"
if errorlevel 1 (
    echo Verification failed - see the errors above.
    echo Check that fmod.dll and fmodstudio.dll in %LIBDIR% match your Python's architecture. See README.md.
    pause & exit /b 1
)

:done
echo.
echo  All set!
echo.
echo  Next up:
echo    1. Make sure the Scripts folder sits next to this Tools folder,
echo       in %%LOCALAPPDATA%%\FMOD Studio\ (serves FMOD 2.02 and 2.03).
echo    2. In FMOD Studio:  Scripts ^> Reload  ^>  DAT-audio Utilities ^> Record Selected Events...
echo.
pause
