; Rewind installer hooks, included by windows/installer.nsi.
; Startup entry lives in HKCU\...\Run under "Rewind"; the app's own
; "Launch on startup" setting reads and writes the same value.

; Append a line to %TEMP%\rewind-uninstall.log (or rewind-install.log), so a
; failed run shows how far it got. Opened and closed per line: the log stays
; complete even if the process is killed or aborts mid-way. Keep double
; quotes out of the text.
!macro REWIND_LOG text
  Push $9
  !ifdef __UNINSTALL__
    FileOpen $9 "$TEMP\rewind-uninstall.log" a
  !else
    FileOpen $9 "$TEMP\rewind-install.log" a
  !endif
  ${If} $9 != ""
    FileSeek $9 0 END
    FileWrite $9 "${text}$\r$\n"
    FileClose $9
  ${EndIf}
  Pop $9
!macroend
!define RewindLog "!insertmacro REWIND_LOG"

; Header: when, which Windows, admin or not, install mode and folder.
!macro REWIND_LOG_START what
  ${GetTime} "" "L" $0 $1 $2 $3 $4 $5 $6
  ReadRegStr $7 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion" "CurrentBuild"
  UserInfo::GetAccountType
  Pop $8
  ${RewindLog} ""
  ${RewindLog} "=== ${what} ${PRODUCTNAME} ${VERSION}: $2-$1-$0 $4:$5:$6 ==="
  ${RewindLog} "windows build $7, account $8, instdir $INSTDIR"
  !if "${INSTALLMODE}" == "both"
    ${RewindLog} "install mode $MultiUser.InstallMode"
  !endif
  ${RewindLog} "command line: $CMDLINE"
!macroend

; Make sure Rewind isn't running before files are replaced or removed: ask it
; to quit on its own (a killed process leaves a dead icon in the tray), wait
; up to 3 s, then force-kill whatever is left. The template's Restart Manager
; check that follows aborts the whole (un)install if it can't stop the app
; itself, which happens with a tray app that hides instead of exiting on
; WM_ENDSESSION, so nothing may be left running by then.
; Clobbers $R0 and $R1.
!macro REWIND_QUIT_RUNNING
  ; 0 = found.
  nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
  Pop $R0
  ${If} $R0 = 0
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ${RewindLog} "rewind is running: asking it to quit"
      ; Exec, not ExecWait: if the --quit instance hangs, the loop below
      ; still gives up and kills it.
      Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
    ${Else}
      ${RewindLog} "rewind is running but $INSTDIR\${MAINBINARYNAME}.exe is missing"
    ${EndIf}
    StrCpy $R1 0
    ${Do}
      Sleep 250
      IntOp $R1 $R1 + 250
      nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
      Pop $R0
      ${If} $R0 <> 0
      ${OrIf} $R1 >= 3000
        ${ExitDo}
      ${EndIf}
    ${Loop}
    ${If} $R0 = 0
      ${RewindLog} "still running after $R1 ms: taskkill /f"
      nsExec::Exec '"$SYSDIR\taskkill.exe" /f /t /im ${MAINBINARYNAME}.exe'
      Pop $R0
      ${RewindLog} "taskkill exit code $R0"
      ; Let Windows release the image file before anything touches it.
      StrCpy $R1 0
      ${Do}
        Sleep 250
        IntOp $R1 $R1 + 250
        nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
        Pop $R0
        ${If} $R0 <> 0
        ${OrIf} $R1 >= 2000
          ${ExitDo}
        ${EndIf}
      ${Loop}
      ${If} $R0 = 0
        ${RewindLog} "rewind is STILL running after taskkill"
      ${Else}
        ${RewindLog} "rewind is gone"
      ${EndIf}
    ${Else}
      ${RewindLog} "rewind quit on its own after $R1 ms"
    ${EndIf}
  ${Else}
    ${RewindLog} "rewind is not running"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro REWIND_LOG_START "install"
  !insertmacro REWIND_QUIT_RUNNING
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LaunchOnStartup == 1
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Rewind" '"$INSTDIR\${MAINBINARYNAME}.exe" --tray'
  ${EndIf}
  ${RewindLog} "install finished"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro REWIND_LOG_START "uninstall"
  !insertmacro REWIND_QUIT_RUNNING
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Rewind"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${RewindLog} "uninstall finished"
!macroend
