; Installer hooks of the per-machine Windows installer (research V12; FR-013).
; mouser-service.exe is the MouserService sidecar, installed beside the app.

!include LogicLib.nsh

; After the files are in place: create and start MouserService, set the
; Ctrl-Alt-Del policy and the firewall rule, and move the installing user's
; desk membership. A step that fails ends the install with its reason.
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$INSTDIR\mouser-service.exe" --install'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP|MB_OK "Mouser could not finish installing:$\r$\n$1" /SD IDOK
    Abort "mouser-service.exe --install: $1"
  ${EndIf}
!macroend

; Before the files go: stop and delete MouserService and end every Mouser
; process, so nothing of Mouser is left running (C:\ProgramData\Mouser,
; the computer's identity and desk, is kept).
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$INSTDIR\mouser-service.exe" --uninstall'
  Pop $0
  Pop $1
!macroend
