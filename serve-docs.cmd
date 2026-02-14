@echo off
REM Serve the docs folder on http://localhost:8000
REM Tries Python first, then npx http-server.
SET PORT=8000
IF EXIST "%~dp0docs" (
  echo Serving docs from %~dp0docs on port %PORT%
) ELSE (
  echo docs folder not found in %~dp0
  pause
  exit /b 1
)
python -V >nul 2>&1
IF %ERRORLEVEL% EQU 0 (
  echo Using python http.server
  pushd "%~dp0docs"
  python -m http.server %PORT%
  popd
  exit /b 0
)
echo Python not found - trying npx http-server (requires Node.js)
npx --version >nul 2>&1
IF %ERRORLEVEL% EQU 0 (
  npx http-server "%~dp0docs" -p %PORT%
  exit /b 0
)
echo Neither Python nor npx available. Install Python 3 or Node.js (npx) or use VS Code Live Server extension.
pause
exit /b 1