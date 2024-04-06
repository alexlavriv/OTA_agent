@echo off
setlocal enabledelayedexpansion
set CI_COMMIT_REF_NAME=%1
set BACKOFFICE_URL=%2
set BACKOFFICE_TOKEN=%3
set VERSION=%4
set CI_COMMIT_TAG=%5

IF "%~5" == "" (
echo CI_COMMIT_TAG is not set, exiting.
exit /b 0
)

:: remove first and last char from the token
set BACKOFFICE_TOKEN=!BACKOFFICE_TOKEN:~1,-1!
echo stripped !BACKOFFICE_TOKEN!

echo ==========
echo %VERSION%
echo ==========

echo CI_COMMIT_REF_NAME: %CI_COMMIT_REF_NAME%
echo BACKOFFICE_URL: %BACKOFFICE_URL%
echo BACKOFFICE_TOKEN: %BACKOFFICE_TOKEN%
echo VERSION: %VERSION%



set /a count=1
for /f "skip=1 delims=:" %%a in ('CertUtil -hashfile "target/release/phantom_agent.exe" SHA1') do (
  if !count! equ 1 set "SHA1=%%a"
  set/a count+=1
)
set "SNAP_FILE_SHA=%SHA1: =%

echo SNAP_FILE_SHA %SNAP_FILE_SHA%
set JSON="{\"component\":\"phantom_agent\",\"version\":\"%VERSION%\",\"link\":\"https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/%CI_COMMIT_REF_NAME%/windows/phantom_agent.exe\",\"checksum\":\"%SNAP_FILE_SHA%\",\"arch\":\"WIN\"}"
echo %JSON%

Set "MyCommand=curl -v POST "%BACKOFFICE_URL%" -o NUL -s -w "\n%%{http_code}\n" -H "accept: application/json" -H "Content-Type: application/json" -d %JSON% -H "Authorization: !BACKOFFICE_TOKEN!""
@for /f %%R in ('%MyCommand%') do ( Set RESPONSE_CODE=%%R )

echo RESPONSE_CODE %RESPONSE_CODE%
SET /A CODE= %RESPONSE_CODE% / 100
echo CODE %CODE%


if NOT %CODE%==2 (exit 1)






