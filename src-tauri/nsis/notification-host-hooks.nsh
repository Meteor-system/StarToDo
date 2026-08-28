; Removes only the notification-host registration owned by this installation.
; The x64 self-contained notification host writes to the 64-bit HKCU Classes view.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode = 1
    Goto startodo_notification_host_cleanup_done
  ${EndIf}

  SetRegView 64
  StrCpy $R0 "$INSTDIR\notification-host\StarToDo.NotificationHost.exe"

  ; The AppUserModelId key is the full host path with forward slashes.
  StrCpy $R1 ""
  StrCpy $R6 0
  startodo_notification_host_aumid_loop:
    StrCpy $R7 "$R0" 1 $R6
    ${If} $R7 S== ""
      Goto startodo_notification_host_aumid_ready
    ${EndIf}
    ${If} $R7 S== "\"
      StrCpy $R7 "/"
    ${EndIf}
    StrCpy $R1 "$R1$R7"
    IntOp $R6 $R6 + 1
    Goto startodo_notification_host_aumid_loop

  startodo_notification_host_aumid_ready:
  StrCpy $R2 "Software\Classes\AppUserModelId\$R1"
  ReadRegStr $R3 HKCU "$R2" "DisplayName"
  ${If} $R3 S!= "StarToDo.NotificationHost"
    Goto startodo_notification_host_cleanup_done
  ${EndIf}

  ReadRegStr $R3 HKCU "$R2" "CustomActivator"

  ; Require a canonical braced CLSID before using it as a registry path segment.
  StrLen $R6 "$R3"
  ${If} $R6 != 38
    Goto startodo_notification_host_cleanup_done
  ${EndIf}
  StrCpy $R7 "$R3" 1 0
  ${If} $R7 S!= "{"
    Goto startodo_notification_host_cleanup_done
  ${EndIf}
  StrCpy $R7 "$R3" 1 37
  ${If} $R7 S!= "}"
    Goto startodo_notification_host_cleanup_done
  ${EndIf}
  StrCpy $R4 "0123456789abcdefABCDEF"
  StrCpy $R6 1

  startodo_notification_host_clsid_loop:
    ${If} $R6 >= 37
      Goto startodo_notification_host_clsid_valid
    ${EndIf}
    ${If} $R6 = 9
    ${OrIf} $R6 = 14
    ${OrIf} $R6 = 19
    ${OrIf} $R6 = 24
      StrCpy $R7 "$R3" 1 $R6
      ${If} $R7 S!= "-"
        Goto startodo_notification_host_cleanup_done
      ${EndIf}
      Goto startodo_notification_host_clsid_next
    ${EndIf}

    StrCpy $R7 "$R3" 1 $R6
    StrCpy $R8 0
    startodo_notification_host_hex_loop:
      StrCpy $R9 "$R4" 1 $R8
      ${If} $R9 S== ""
        Goto startodo_notification_host_cleanup_done
      ${EndIf}
      ${If} $R7 S== $R9
        Goto startodo_notification_host_clsid_next
      ${EndIf}
      IntOp $R8 $R8 + 1
      Goto startodo_notification_host_hex_loop

  startodo_notification_host_clsid_next:
    IntOp $R6 $R6 + 1
    Goto startodo_notification_host_clsid_loop

  startodo_notification_host_clsid_valid:
  StrCpy $R4 "Software\Classes\CLSID\$R3\LocalServer32"
  ReadRegStr $R5 HKCU "$R4" ""

  ; Accept only the exact representations written by the notification toolkit.
  StrCmp $R5 "$R0" startodo_notification_host_owned
  StrCmp $R5 "$\"$R0$\"" startodo_notification_host_owned
  StrCmp $R5 "$\"$R0$\" -ToastActivated" startodo_notification_host_owned
  Goto startodo_notification_host_cleanup_done

  startodo_notification_host_owned:
  DeleteRegKey HKCU "Software\Classes\CLSID\$R3"
  DeleteRegKey HKCU "$R2"

  startodo_notification_host_cleanup_done:
!macroend
