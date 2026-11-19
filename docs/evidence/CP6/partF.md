# Checkpoint 6 Part F: Packaging and Build Verification

## 1. Per-User NSIS Configuration (`tauri.conf.json`)

Tauri bundle configuration is verified in `crates/ruby-desktop/tauri.conf.json`:
```json
"bundle": {
  "active": true,
  "targets": "all",
  "windows": {
    "certificateThumbprint": null,
    "digestAlgorithm": "sha256",
    "timestampUrl": "",
    "nsis": {
      "installMode": "currentUser",
      "installerHooks": "nsis-hooks.nsi"
    }
  }
}
```
- `installMode: "currentUser"` ensures per-user installation without requiring administrative privileges or UAC elevation.
- Standard destination defaults to `%LOCALAPPDATA%\Programs\Ruby`.

## 2. Uninstallation Lifecycle Hooks (`nsis-hooks.nsi`)

The custom NSIS hook script (`crates/ruby-desktop/nsis-hooks.nsi`) defines the `customUnInstall` macro:
```nsis
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
```
- **Autostart Cleanup**: Safely removes `Ruby` value from `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` without requiring admin rights.
- **User Confirmation on Data Deletion**: Prompts the user with `MB_YESNO|MB_ICONQUESTION` before deleting `%LOCALAPPDATA%\Ruby` containing memory DB, audit logs, and config. If the user clicks "No", local data is preserved.

## 3. Release Build Verification (`cargo build --release`)

Release build command executed:
`cargo build --release -p ruby-desktop`

Output:
```
Finished `release` profile [optimized] target(s) in 10m 45s
```

Artifact details:
- **Binary Path**: `D:\projects\ruby-agent\target\release\ruby-desktop.exe`
- **File Size**: `9,627,136 bytes` (~9.18 MB)
- **Status**: Successful compilation without errors.

## 4. Safety Constraint Compliance
- No installer or binary was executed.
- No Windows registry keys were modified during build or testing.
- Local-only, CI not run.
