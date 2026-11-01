; Rewind installer hooks, included by windows/installer.nsi.
; Startup entry lives in HKCU\...\Run under "Rewind"; the app's own
; "Launch on startup" setting reads and writes the same value.

; Ask a running Rewind to quit on its own before the template's running-app
; check kills it: a killed process leaves a dead icon in the tray.
!macro REWIND_QUIT_RUNNING
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 +3
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
    Sleep 2000
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro REWIND_QUIT_RUNNING
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LaunchOnStartup == 1
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Rewind" '"$INSTDIR\${MAINBINARYNAME}.exe" --tray'
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro REWIND_QUIT_RUNNING
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Rewind"
!macroend
