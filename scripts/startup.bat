powershell Start-Process SCHTASKS /CREATE /F /SC ONLOGON /TN alex_test /TR notepad /RL HIGHEST  -Verb runAs
pause