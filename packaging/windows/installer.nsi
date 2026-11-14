; OpenVysor installer for Windows.
;
; Installs for the current user only, so no administrator rights are needed, into
; %LOCALAPPDATA%\Programs\OpenVysor. It adds Start menu and optional desktop shortcuts,
; and an entry in Settings > Apps > Installed apps that runs the uninstaller.
;
; Build from the repository root after `cargo build --release`:
;   makensis /DVERSION=0.4.0 /DSOURCE=native\target\release\openvysor-native.exe ^
;            /DOUTFILE=dist-release\OpenVysor-windows-x64-setup.exe packaging\windows\installer.nsi

; makensis runs from this script's folder, so the paths below are relative to it.
Unicode true
SetCompressor /SOLID lzma
ManifestDPIAware true

!ifndef VERSION
  !error "Pass /DVERSION=x.y.z"
!endif
!ifndef SOURCE
  !error "Pass /DSOURCE=path\to\openvysor-native.exe"
!endif
!ifndef OUTFILE
  !define OUTFILE "OpenVysor-windows-x64-setup.exe"
!endif

!define NAME "OpenVysor"
!define EXE "OpenVysor.exe"
!define PUBLISHER "SahinurDEV"
!define WEBSITE "https://sahinurdev.github.io/OpenVysor/"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}"
; Settings, the scrcpy server and adb downloaded by the app live here, apart from the program.
!define DATA_DIR "$LOCALAPPDATA\OpenVysor"

Name "${NAME}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${NAME}"
InstallDirRegKey HKCU "Software\${NAME}" "InstallDir"
RequestExecutionLevel user
BrandingText "${NAME} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${NAME} installer"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "Apache-2.0"

!include "MUI2.nsh"
!include "LogicLib.nsh"

!define MUI_ICON ".\openvysor.ico"
!define MUI_UNICON ".\openvysor.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Open ${NAME} now"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE ".\..\..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_COMPONENTS
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; A running copy locks its own file, so it is closed before files are replaced or removed.
!macro CloseRunningApp
  nsExec::Exec 'taskkill /F /IM "${EXE}"'
  Pop $0
  Sleep 500
!macroend

Section "${NAME} (required)" SecApp
  SectionIn RO
  !insertmacro CloseRunningApp
  SetOutPath "$INSTDIR"
  File "/oname=${EXE}" "${SOURCE}"
  File "/oname=openvysor.ico" ".\openvysor.ico"
  File "/oname=LICENSE.txt" ".\..\..\LICENSE"
  File "/oname=NOTICE.txt" ".\..\..\NOTICE"
  File "/oname=scrcpy-LICENSE.txt" ".\..\..\native\resources\scrcpy-LICENSE.txt"
  WriteUninstaller "$INSTDIR\Uninstall ${NAME}.exe"

  CreateShortcut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\openvysor.ico"

  WriteRegStr HKCU "Software\${NAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "${WEBSITE}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\openvysor.ico"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall ${NAME}.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall ${NAME}.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\${NAME}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\openvysor.ico"
SectionEnd

LangString DESC_SecApp ${LANG_ENGLISH} "The ${NAME} program and a Start menu shortcut."
LangString DESC_SecDesktop ${LANG_ENGLISH} "A shortcut on the desktop."
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} $(DESC_SecApp)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} $(DESC_SecDesktop)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "un.${NAME}" UnSecApp
  SectionIn RO
  !insertmacro CloseRunningApp
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\openvysor.ico"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\NOTICE.txt"
  Delete "$INSTDIR\scrcpy-LICENSE.txt"
  Delete "$INSTDIR\Uninstall ${NAME}.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${NAME}.lnk"
  Delete "$DESKTOP\${NAME}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "Software\${NAME}"
SectionEnd

; Off by default, so reinstalling later keeps settings, workspaces and key mappings.
Section /o "un.Settings and downloaded ADB" UnSecData
  ; adb installed by the app may still be running as a server and would lock its folder.
  ${If} ${FileExists} "${DATA_DIR}\platform-tools\adb.exe"
    nsExec::Exec '"${DATA_DIR}\platform-tools\adb.exe" kill-server'
    Pop $0
  ${EndIf}
  RMDir /r "${DATA_DIR}"
SectionEnd

LangString DESC_UnSecApp ${LANG_ENGLISH} "Removes the program and its shortcuts."
LangString DESC_UnSecData ${LANG_ENGLISH} "Also deletes settings, workspaces, key mappings and the ADB copy that ${NAME} downloaded. Screenshots and recordings are kept."
!insertmacro MUI_UNFUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${UnSecApp} $(DESC_UnSecApp)
  !insertmacro MUI_DESCRIPTION_TEXT ${UnSecData} $(DESC_UnSecData)
!insertmacro MUI_UNFUNCTION_DESCRIPTION_END
