; NSIS installer hooks for Wave Desktop.
;
; Adds the install directory to the per-user PATH on install (so `wave-cli` is
; globally callable in cmd/PowerShell), and removes it on uninstall. The actual
; registry work is done by the bundled wave-cli itself (`wave-cli path ...`),
; which handles dedup and preserves REG_EXPAND_SZ variables — far more robust
; than NSIS string surgery.

!macro NSIS_HOOK_POSTINSTALL
  ; wave-cli.exe is installed next to the app; add $INSTDIR to the user PATH.
  nsExec::Exec '"$INSTDIR\wave-cli.exe" path install --dir "$INSTDIR" -q'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Runs before files are removed, so wave-cli.exe still exists to clean PATH.
  nsExec::Exec '"$INSTDIR\wave-cli.exe" path uninstall --dir "$INSTDIR" -q'
!macroend
