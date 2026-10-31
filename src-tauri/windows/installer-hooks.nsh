; Rewind installer hooks, included by windows/installer.nsi.
; Startup entry lives in HKCU\...\Run under "Rewind"; the app's own
; "Launch on startup" setting reads and writes the same value.

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LaunchOnStartup == 1
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Rewind" '"$INSTDIR\${MAINBINARYNAME}.exe" --tray'
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Rewind"
!macroend
