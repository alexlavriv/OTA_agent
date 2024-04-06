 @echo off
 setlocal enabledelayedexpansion

 rustup update
 rustc --version
 cargo clean


 rmdir /Q /S target
 mkdir target\wix


 powershell "Start-Process -Wait -Verb RunAs powershell '-NoProfile iwr https://releases.jfrog.io/artifactory/jfrog-cli/v2-jf/[RELEASE]/jfrog-cli-windows-amd64/jf.exe -OutFile $env:SYSTEMROOT\system32\jf.exe'"
 jf c add --artifactory-url %ARTIFACTORY_URL% --user %ARTIFACTORY_USER% --password=%ARTIFACTORY_PASS% --interactive=false


 cargo wix --nocapture
if %errorlevel% neq 0 exit /b %errorlevel%

 cd target\wix
 jf rt u *.msi Phantom.Binary/SDK-Phantom-Agent/%CI_COMMIT_REF_NAME%/%ARCH%/
 aws s3 cp ./projs/oden_net_sim/target/debian/ s3://phau-artifactory-eng2/Phantom.Binary/SDK-Phantom-Agent/%CI_COMMIT_REF_NAME%/%ARCH%/ --recursive --exclude "*" --include "*.msi"

 cd ..\release
 jf rt u phantom_agent.exe Phantom.Binary/SDK-Phantom-Agent/%CI_COMMIT_REF_NAME%/%ARCH%/
 aws s3 cp windows_service.exe s3://phau-artifactory-eng2/Phantom.Binary/SDK-Phantom-Agent/%CI_COMMIT_REF_NAME%/%ARCH%/

 :: Only register tag for tags
 if "%CI_COMMIT_TAG%"=="" exit /b 0
 cd ..\..

 call .\scripts\register_tag.bat %CI_COMMIT_REF_NAME% %BACKOFFICE_URL_IL% "%BACKOFFICE_TOKEN%" %CI_COMMIT_REF_NAME% %CI_COMMIT_TAG%
 call .\scripts\register_tag.bat %CI_COMMIT_REF_NAME% %BACKOFFICE_URL_QA% "%BACKOFFICE_TOKEN%" %CI_COMMIT_REF_NAME% %CI_COMMIT_TAG%
 call .\scripts\register_tag.bat %CI_COMMIT_REF_NAME% %BACKOFFICE_URL_PROD% "%BACKOFFICE_TOKEN%" %CI_COMMIT_REF_NAME% %CI_COMMIT_TAG%
 call .\scripts\register_tag.bat %CI_COMMIT_REF_NAME% %BACKOFFICE_URL_ENG% "%BACKOFFICE_TOKEN_ENG%" %CI_COMMIT_REF_NAME% %CI_COMMIT_TAG%
 call .\scripts\register_tag.bat %CI_COMMIT_REF_NAME% %BACKOFFICE_URL_OPT2% "%BACKOFFICE_TOKEN_OPS2_BEARER%" %CI_COMMIT_REF_NAME% %CI_COMMIT_TAG%
