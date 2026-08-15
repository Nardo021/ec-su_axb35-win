; ec-su_axb35-win Installer

!define PRODUCT_NAME "ec-su_axb35-win"
!define PRODUCT_VERSION "2.1.0"
!define PRODUCT_PUBLISHER "deseven"
!define PRODUCT_WEB_SITE "https://github.com/deseven/ec-su_axb35-win"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\evox2-control.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_UNINST_ROOT_KEY "HKLM"

!define SERVICE_NAME "ec-su_axb35-win"
!define SERVICE_DISPLAY_NAME "EVO-X2 Control"
!define SERVICE_DESCRIPTION "Optional background service for EVO-X2 / SU_AXB35 EC control"

; Installation directories
!define INSTALL_DIR "$PROGRAMFILES64\ec-su_axb35-win"

; Include required headers
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "WinMessages.nsh"

; MUI Settings
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

; Welcome page
!insertmacro MUI_PAGE_WELCOME
; License page (optional - uncomment if you have a license file)
; !insertmacro MUI_PAGE_LICENSE "license.txt"
; Directory page
!insertmacro MUI_PAGE_DIRECTORY
; Instfiles page
!insertmacro MUI_PAGE_INSTFILES
; Finish page with option to run client
!define MUI_FINISHPAGE_RUN "$INSTDIR\evox2-control.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Run EVO-X2 Control"
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_INSTFILES

; Language files
!insertmacro MUI_LANGUAGE "English"

; Reserve files (MUI_RESERVEFILE_INSTALLOPTIONS is deprecated in MUI2)
; Use ReserveFile if needed for specific plugins

; MUI end ------

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "ec-su_axb35-win-installer-${PRODUCT_VERSION}.exe"
InstallDir "${INSTALL_DIR}"
InstallDirRegKey HKLM "${PRODUCT_DIR_REGKEY}" ""
ShowInstDetails show
ShowUnInstDetails show
RequestExecutionLevel admin

; Version Information
VIProductVersion "2.1.0.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "Comments" "EC SU_AXB35 WIN Installer"
VIAddVersionKey "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey "LegalTrademarks" ""
VIAddVersionKey "LegalCopyright" "© ${PRODUCT_PUBLISHER}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Installer"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"

Function .onInit
  ; This is important to have $APPDATA variable
  ; point to ProgramData folder instead of current user's Roaming folder
  SetShellVarContext all
  
  ; Check if running as administrator
  UserInfo::GetAccountType
  pop $0
  ${If} $0 != "admin"
    MessageBox MB_ICONSTOP "Administrator rights required!"
    SetErrorLevel 740 ; ERROR_ELEVATION_REQUIRED
    Quit
  ${EndIf}

  ReadRegStr $1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO" "DisplayVersion"
  ${If} $1 == ""
    MessageBox MB_ICONEXCLAMATION "PawnIO was not detected.$\r$\n$\r$\nInstall the official signed PawnIO release from https://pawnio.eu/ before using EVO-X2 Control.$\r$\n$\r$\nSecure Boot can remain enabled. This installer does not bundle a kernel driver."
  ${EndIf}
FunctionEnd

; Service management functions
Function StopExistingService
  DetailPrint "Checking for existing service..."
  
  ; Stop the service if it's running
  nsExec::ExecToLog 'sc query "${SERVICE_NAME}"'
  Pop $0
  ${If} $0 == 0
    DetailPrint "Stopping existing service..."
    nsExec::ExecToLog 'sc stop "${SERVICE_NAME}"'
    Sleep 3000 ; Wait 3 seconds for service to stop
  ${EndIf}
FunctionEnd

Function KillExistingClientProcess
  DetailPrint "Checking for existing app processes..."
  nsExec::ExecToLog 'taskkill /F /IM evox2-control.exe'
  nsExec::ExecToLog 'taskkill /F /IM evox2ctl.exe'
  nsExec::ExecToLog 'taskkill /F /IM ec-su_axb35-win-client.exe'
  nsExec::ExecToLog 'taskkill /F /IM ec-su_axb35-server.exe'
  Sleep 1000
FunctionEnd

Function RemoveExistingService
  DetailPrint "Removing leftover background service if present..."
  nsExec::ExecToLog 'sc stop "${SERVICE_NAME}"'
  Sleep 2000
  nsExec::ExecToLog 'sc delete "${SERVICE_NAME}"'
FunctionEnd


Section "MainSection" SEC01
  ; Stop existing service if running
  Call StopExistingService
  
  ; Kill existing client process if running
  Call KillExistingClientProcess
  
  ; Create installation directory
  SetOutPath "$INSTDIR"
  SetOverwrite ifnewer
  
  DetailPrint "Installing EVO-X2 Control..."
  File "target\release\evox2-control.exe"
  File "/oname=evox2ctl.exe" "target\release\evox2-control.exe"
  
  ; Create scripts directory and install scripts
  DetailPrint "Installing scripts..."
  CreateDirectory "$APPDATA\ec-su_axb35-win\scripts"
  SetOutPath "$APPDATA\ec-su_axb35-win\scripts"
  File "server\scripts\info.ps1"
  File "server\scripts\test_fan_mode_fixed.ps1"
  
  DetailPrint "This build is a single GUI app. No background service is installed."
  Call RemoveExistingService
SectionEnd

Section -AdditionalIcons
  SetOutPath $INSTDIR
  WriteIniStr "$INSTDIR\${PRODUCT_NAME}.url" "InternetShortcut" "URL" "${PRODUCT_WEB_SITE}"
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\EVO-X2 Control.lnk" "$INSTDIR\evox2-control.exe"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Quiet.lnk" "$INSTDIR\evox2ctl.exe" "mode quiet"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Balanced.lnk" "$INSTDIR\evox2ctl.exe" "mode balanced"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Performance.lnk" "$INSTDIR\evox2ctl.exe" "mode performance"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Website.lnk" "$INSTDIR\${PRODUCT_NAME}.url"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\uninst.exe"
SectionEnd

Section -Post
  WriteUninstaller "$INSTDIR\uninst.exe"
  WriteRegStr HKLM "${PRODUCT_DIR_REGKEY}" "" "$INSTDIR\evox2-control.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayName" "$(^Name)"
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
  ; This is important to have $APPDATA variable
  ; point to ProgramData folder instead of current user's Roaming folder
  SetShellVarContext all
  
  ; Check if running as administrator
  UserInfo::GetAccountType
  pop $0
  ${If} $0 != "admin"
    MessageBox MB_ICONSTOP "Administrator rights required!"
    SetErrorLevel 740 ; ERROR_ELEVATION_REQUIRED
    Quit
  ${EndIf}
  
  MessageBox MB_ICONQUESTION|MB_YESNO|MB_DEFBUTTON2 "Are you sure you want to completely remove $(^Name) and all of its components?" IDYES +2
  Abort
FunctionEnd

Section Uninstall
  ; Stop and remove service
  DetailPrint "Stopping and removing service..."
  nsExec::ExecToLog 'sc stop "${SERVICE_NAME}"'
  Sleep 3000
  nsExec::ExecToLog 'sc delete "${SERVICE_NAME}"'
  Sleep 1000
  
  nsExec::ExecToLog 'taskkill /F /IM evox2-control.exe'
  nsExec::ExecToLog 'taskkill /F /IM evox2ctl.exe'
  nsExec::ExecToLog 'taskkill /F /IM ec-su_axb35-win-client.exe'
  Sleep 1000
  
  ; Remove files
  Delete "$INSTDIR\${PRODUCT_NAME}.url"
  Delete "$INSTDIR\uninst.exe"
  Delete "$INSTDIR\ec-su_axb35-server.exe"
  Delete "$INSTDIR\evox2-control.exe"
  Delete "$INSTDIR\evox2ctl.exe"
  Delete "$INSTDIR\ec-su_axb35-win-client.exe"
  
  ; Remove scripts files
  Delete "$APPDATA\ec-su_axb35-win\scripts\info.ps1"
  Delete "$APPDATA\ec-su_axb35-win\scripts\test_fan_mode_fixed.ps1"
  RMDir "$APPDATA\ec-su_axb35-win\scripts"
  
  RMDir "$APPDATA\ec-su_axb35-win"
  
  ; Remove shortcuts
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\EVO-X2 Control.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\EC SU_AXB35 Client.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Quiet.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Balanced.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Performance.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Website.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"
  
  ; Remove installation directory
  RMDir "$INSTDIR"
  
  ; Remove registry keys
  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKLM "${PRODUCT_DIR_REGKEY}"
  SetAutoClose true
SectionEnd