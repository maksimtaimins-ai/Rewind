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

; Header: when, which Windows (11 = build 22000+), elevated or not, install
; mode and folder. "account" is group membership, "elevated" is the token.
!macro REWIND_LOG_START what
  ${GetTime} "" "L" $0 $1 $2 $3 $4 $5 $6
  ReadRegStr $7 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion" "CurrentBuild"
  StrCpy $9 "Windows 10 or older"
  ${If} $7 >= 22000
    StrCpy $9 "Windows 11"
  ${EndIf}
  UserInfo::GetAccountType
  Pop $8
  !ifdef __UNINSTALL__
    Call un.RewindIsElevated
  !else
    Call RewindIsElevated
  !endif
  ${RewindLog} ""
  ${RewindLog} "=== ${what} ${PRODUCTNAME} ${VERSION}: $2-$1-$0 $4:$5:$6 ==="
  ${RewindLog} "$9 (build $7), account $8, elevated $RewindRes, instdir $INSTDIR"
  !if "${INSTALLMODE}" == "both"
    ${RewindLog} "install mode $MultiUser.InstallMode"
  !endif
  ${RewindLog} "command line: $CMDLINE"
!macroend

Var RewindRes

; The shared stop-Rewind code, defined once for the installer and once for the
; uninstaller ("" and "un."). Insert after the template's Vars, before onInit.
;
;   RewindIsElevated: $RewindRes = 1 if this process has an elevated token.
;   RewindStopApp:    $RewindRes = 1 once Rewind is gone, 0 if the user gave up
;                     (or nobody could be asked, in silent and passive runs).
;
; Order: ask it to quit (a killed process leaves a dead icon in the tray),
; wait up to 3 s, taskkill /f /t, wait up to 2 s for the image file to be
; released, then kill leftover WebView2 helpers of this app. Still alive:
; Retry / Cancel dialog. All registers are preserved.
!macro REWIND_DEFINE_FUNCS un
Function ${un}RewindIsElevated
  Push $0
  Push $1
  Push $2
  StrCpy $RewindRes 0
  ; TokenElevation = 20. A failed call leaves 0.
  System::Call 'advapi32::OpenProcessToken(p -1, i 8, *p .r0) i .r1'
  ${If} $1 <> 0
    System::Call 'advapi32::GetTokenInformation(p r0, i 20, *i .r2, i 4, *i .r1) i .r1'
    ${If} $1 <> 0
      StrCpy $RewindRes $2
    ${EndIf}
    System::Call 'kernel32::CloseHandle(p r0)'
  ${EndIf}
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

Function ${un}RewindStopApp
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  StrCpy $RewindRes 1
  StrCpy $5 0 ; 1 once we found it running
  ${Do}
    ; 0 = found.
    nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 <> 0
      ${RewindLog} "rewind is not running"
      ${ExitDo}
    ${EndIf}
    StrCpy $5 1

    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ${RewindLog} "rewind is running: asking it to quit"
      ; Exec, not ExecWait: a hung --quit instance must not hang us.
      Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
    ${Else}
      ${RewindLog} "rewind is running but $INSTDIR\${MAINBINARYNAME}.exe is missing"
    ${EndIf}
    StrCpy $1 0
    ${Do}
      Sleep 250
      IntOp $1 $1 + 250
      nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
      Pop $0
      ${If} $0 <> 0
      ${OrIf} $1 >= 3000
        ${ExitDo}
      ${EndIf}
    ${Loop}
    ${If} $0 <> 0
      ${RewindLog} "rewind quit on its own after $1 ms"
      ${ExitDo}
    ${EndIf}

    ${RewindLog} "still running after $1 ms: taskkill /f /t"
    nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /f /t /im ${MAINBINARYNAME}.exe'
    Pop $3 ; exit code
    Pop $4 ; output
    ${RewindLog} "taskkill exit code $3, output: $4"
    StrCpy $1 0
    ${Do}
      Sleep 250
      IntOp $1 $1 + 250
      nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
      Pop $0
      ${If} $0 <> 0
      ${OrIf} $1 >= 2000
        ${ExitDo}
      ${EndIf}
    ${Loop}
    ${If} $0 <> 0
      ${RewindLog} "rewind is gone after taskkill"
      ${ExitDo}
    ${EndIf}

    ; Still alive. taskkill failing while the process is visible is what a
    ; UAC or integrity mismatch looks like (Rewind elevated, this not, or the
    ; reverse); Win11 enforces it strictly. Exit code 128 = already gone.
    ${If} $3 <> 0
    ${AndIf} $3 <> 128
      ${RewindLog} "rewind survived a failed taskkill: different permissions?"
      StrCpy $2 "Rewind is running with different permissions than this installer, so it can't be closed from here. Please close it manually (tray icon or Task Manager, 'Run as administrator' if needed), then click Retry."
    ${Else}
      ${RewindLog} "rewind is STILL running after taskkill"
      StrCpy $2 "Rewind is running and can't be closed automatically. Please close it via the tray icon or Task Manager, then click Retry."
    ${EndIf}
    ${If} ${Silent}
    ${OrIf} $PassiveMode = 1
      ${RewindLog} "no UI (silent or passive): giving up"
      StrCpy $RewindRes 0
      ${ExitDo}
    ${EndIf}
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION $2 IDRETRY user_retry
    ${RewindLog} "user cancelled"
    StrCpy $RewindRes 0
    ${ExitDo}
    user_retry:
    ${RewindLog} "user chose retry"
  ${Loop}

  ; No PowerShell / WebView2-helper kill here on purpose: an unsigned
  ; installer that runs "powershell -ExecutionPolicy Bypass ... Stop-Process
  ; -Force" is scored as malware by Defender's ML (Trojan:Win32/Bearfoos.A!ml).
  ; Orphaned WebView2 helpers exit by themselves once the app is gone.

  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd
!macroend

; Stop Rewind or abort. Used by the install and uninstall hooks, and before
; the installer runs an older uninstaller. Clobbers $0.
!macro REWIND_QUIT_RUNNING
  !ifdef __UNINSTALL__
    Call un.RewindStopApp
  !else
    Call RewindStopApp
  !endif
  ${If} $RewindRes = 0
    ${RewindLog} "aborting: rewind could not be stopped"
    Abort "Rewind is still running. Close it and run this again."
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
