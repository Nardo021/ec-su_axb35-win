; ec-su_axb35-win installer (single-process GUI + CLI)

!define PRODUCT_NAME "ec-su_axb35-win"
!define PRODUCT_DISPLAY_NAME "ec-su_axb35-win"
!define PRODUCT_VERSION "2.3.1"
!define PRODUCT_PUBLISHER "Nardo021"
!define PRODUCT_WEB_SITE "https://github.com/Nardo021/ec-su_axb35-win"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\evox2-control.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_UNINST_ROOT_KEY "HKLM"
!define LEGACY_SERVICE_NAME "ec-su_axb35-win"
!define INSTALL_DIR "$PROGRAMFILES64\ec-su_axb35-win"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "WinMessages.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\evox2-control.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Run ${PRODUCT_NAME}"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_RESERVEFILE_LANGDLL

Name "${PRODUCT_DISPLAY_NAME} ${PRODUCT_VERSION}"
OutFile "ec-su_axb35-win-installer-${PRODUCT_VERSION}.exe"
InstallDir "${INSTALL_DIR}"
InstallDirRegKey HKLM "${PRODUCT_DIR_REGKEY}" ""
ShowInstDetails show
ShowUnInstDetails show
RequestExecutionLevel admin

VIProductVersion "2.3.1.0"
VIAddVersionKey "ProductName" "${PRODUCT_DISPLAY_NAME}"
VIAddVersionKey "Comments" "Single-process GUI for SU_AXB35 EC control"
VIAddVersionKey "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey "LegalTrademarks" ""
VIAddVersionKey "LegalCopyright" "© deseven, ${PRODUCT_PUBLISHER}"
VIAddVersionKey "FileDescription" "${PRODUCT_DISPLAY_NAME} Installer"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"

Function .onInit
  !insertmacro MUI_LANGDLL_DISPLAY
  SetShellVarContext all
  UserInfo::GetAccountType
  pop $0
  ${If} $0 != "admin"
    MessageBox MB_ICONSTOP "Administrator rights required!"
    SetErrorLevel 740
    Quit
  ${EndIf}

  ReadRegStr $1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO" "DisplayVersion"
  ${If} $1 == ""
    MessageBox MB_ICONEXCLAMATION "PawnIO was not detected.$\r$\n$\r$\nInstall the official signed PawnIO release from https://pawnio.eu/ before using ${PRODUCT_NAME}.$\r$\n$\r$\nSecure Boot can remain enabled. This installer does not bundle a kernel driver."
  ${EndIf}
FunctionEnd

Function RemoveLegacyService
  DetailPrint "Removing leftover Windows service from older builds if present..."
  nsExec::ExecToLog 'sc query "${LEGACY_SERVICE_NAME}"'
  Pop $0
  ${If} $0 == 0
    nsExec::ExecToLog 'sc stop "${LEGACY_SERVICE_NAME}"'
    Sleep 2000
  ${EndIf}
  nsExec::ExecToLog 'sc delete "${LEGACY_SERVICE_NAME}"'
FunctionEnd

Function KillExistingApp
  DetailPrint "Stopping running ${PRODUCT_NAME} processes..."
  nsExec::ExecToLog 'taskkill /F /IM evox2-control.exe'
  nsExec::ExecToLog 'taskkill /F /IM evox2ctl.exe'
  nsExec::ExecToLog 'taskkill /F /IM ec-su_axb35-win-client.exe'
  nsExec::ExecToLog 'taskkill /F /IM ec-su_axb35-server.exe'
  Sleep 1000
FunctionEnd

Section "MainSection" SEC01
  Call RemoveLegacyService
  Call KillExistingApp

  SetOutPath "$INSTDIR"
  SetOverwrite ifnewer
  DetailPrint "Installing ${PRODUCT_NAME} (single-process GUI)..."
  File "target\release\evox2-control.exe"
  File "/oname=evox2ctl.exe" "target\release\evox2-control.exe"
  File "LICENSE"

  DetailPrint "Installing scripts..."
  CreateDirectory "$APPDATA\ec-su_axb35-win\scripts"
  SetOutPath "$APPDATA\ec-su_axb35-win\scripts"
  File "server\scripts\info.ps1"
  File "server\scripts\test_fan_mode_fixed.ps1"
SectionEnd

Section -AdditionalIcons
  SetOutPath $INSTDIR
  WriteIniStr "$INSTDIR\${PRODUCT_NAME}.url" "InternetShortcut" "URL" "${PRODUCT_WEB_SITE}"
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\evox2-control.exe"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Quiet.lnk" "$INSTDIR\evox2ctl.exe" "mode quiet"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Balanced.lnk" "$INSTDIR\evox2ctl.exe" "mode balanced"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Performance.lnk" "$INSTDIR\evox2ctl.exe" "mode performance"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Website.lnk" "$INSTDIR\${PRODUCT_NAME}.url"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\uninst.exe"
SectionEnd

Section -Post
  WriteUninstaller "$INSTDIR\uninst.exe"
  WriteRegStr HKLM "${PRODUCT_DIR_REGKEY}" "" "$INSTDIR\evox2-control.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayName" "${PRODUCT_DISPLAY_NAME}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "UninstallString" "$INSTDIR\uninst.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\evox2-control.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
SectionEnd

Function un.onUninstSuccess
  HideWindow
  MessageBox MB_ICONINFORMATION|MB_OK "$(^Name) was successfully removed from your computer."
FunctionEnd

Function un.onInit
  SetShellVarContext all
  UserInfo::GetAccountType
  pop $0
  ${If} $0 != "admin"
    MessageBox MB_ICONSTOP "Administrator rights required!"
    SetErrorLevel 740
    Quit
  ${EndIf}
  MessageBox MB_ICONQUESTION|MB_YESNO|MB_DEFBUTTON2 "Are you sure you want to completely remove $(^Name) and all of its components?" IDYES +2
  Abort
FunctionEnd

Section Uninstall
  DetailPrint "Removing leftover Windows service from older builds if present..."
  nsExec::ExecToLog 'sc stop "${LEGACY_SERVICE_NAME}"'
  Sleep 2000
  nsExec::ExecToLog 'sc delete "${LEGACY_SERVICE_NAME}"'
  Sleep 1000

  nsExec::ExecToLog 'taskkill /F /IM evox2-control.exe'
  nsExec::ExecToLog 'taskkill /F /IM evox2ctl.exe'
  nsExec::ExecToLog 'taskkill /F /IM ec-su_axb35-win-client.exe'
  Sleep 1000

  Delete "$INSTDIR\${PRODUCT_NAME}.url"
  Delete "$INSTDIR\uninst.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\ec-su_axb35-server.exe"
  Delete "$INSTDIR\evox2-control.exe"
  Delete "$INSTDIR\evox2ctl.exe"
  Delete "$INSTDIR\ec-su_axb35-win-client.exe"

  Delete "$APPDATA\ec-su_axb35-win\scripts\info.ps1"
  Delete "$APPDATA\ec-su_axb35-win\scripts\test_fan_mode_fixed.ps1"
  RMDir "$APPDATA\ec-su_axb35-win\scripts"
  RMDir "$APPDATA\ec-su_axb35-win"

  Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\EVO-X2 Control.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\EC SU_AXB35 Client.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Quiet.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Balanced.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Performance.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Website.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  RMDir "$INSTDIR"

  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKLM "${PRODUCT_DIR_REGKEY}"
  SetAutoClose true
SectionEnd
