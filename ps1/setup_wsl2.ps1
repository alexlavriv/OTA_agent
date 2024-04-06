Write-Host "Installing WSL"

Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass -force

dism.exe /online /enable-feature /featurename:Microsoft-Windows-Subsystem-Linux /all /norestart
dism.exe /online /enable-feature /featurename:VirtualMachinePlatform /all /norestart