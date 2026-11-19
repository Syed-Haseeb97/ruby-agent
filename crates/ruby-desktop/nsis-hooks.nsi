; Custom NSIS Installer & Uninstaller Hooks for Ruby Assistant
; Configured for Per-User Installation without Admin Escalation

!macro customInstall
  ; Per-user install actions if needed
!macroend

!macro customUnInstall
  ; 1. Unregister Windows autostart Run key from CurrentUser registry
  DetailPrint "Removing Ruby autostart entry from CurrentUser Run key..."
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Ruby"

  ; 2. Prompt before deleting %LOCALAPPDATA%\Ruby data directory (memory, logs, config)
  MessageBox MB_YESNO|MB_ICONQUESTION "Do you want to permanently delete your Ruby database, logs, and settings stored in $LOCALAPPDATA\Ruby?" IDNO skip_user_data_cleanup
    DetailPrint "Deleting Ruby application data directory: $LOCALAPPDATA\Ruby..."
    RMDir /r "$LOCALAPPDATA\Ruby"
  skip_user_data_cleanup:
!macroend
