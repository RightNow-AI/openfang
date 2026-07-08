@echo off
REM FreEco.ai portable launcher — Windows
REM Run from a USB drive or any folder: all config/data stays alongside this
REM script (in .\data), so the drive is fully self-contained and portable
REM across machines.

setlocal
set "ROOT=%~dp0"
set "OPENFANG_HOME=%ROOT%data"
if "%OPENFANG_LISTEN%"=="" set "OPENFANG_LISTEN=127.0.0.1:4200"

if /I "%PROCESSOR_ARCHITECTURE%"=="ARM64" (
    set "ARCH=aarch64"
) else (
    set "ARCH=x86_64"
)

set "EXE=%ROOT%bin\windows\%ARCH%\openfang.exe"
if not exist "%EXE%" (
    echo   Could not find a FreEco.ai binary for windows\%ARCH% at:
    echo     %EXE%
    pause
    exit /b 1
)

if not exist "%OPENFANG_HOME%" mkdir "%OPENFANG_HOME%"
if not exist "%OPENFANG_HOME%\config.toml" (
    echo   First run - starting FreEco.ai setup wizard...
    "%EXE%" init
)

echo.
echo   FreEco.ai starting - dashboard: http://%OPENFANG_LISTEN%
echo   Data directory: %OPENFANG_HOME%
echo   Close this window to stop FreEco.ai.
echo.

REM Open the dashboard a few seconds after launch, once the server has
REM had time to bind its port. Runs in a detached shell so it doesn't
REM block the foreground server below.
start "" cmd /c "timeout /t 3 >nul & start "" "http://%OPENFANG_LISTEN%""

"%EXE%" start
