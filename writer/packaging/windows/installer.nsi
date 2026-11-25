; Sloth Writer's Windows installer (NSIS). Built by .github/workflows/release.yml:
;   makensis /DVERSION=2.5.0 /DEXE=path\to\sloth-writer.exe /DOUT=Setup.exe installer.nsi
; Installs for you alone (no administrator needed) in your own Programs
; folder, with a Start menu shortcut and an entry in Settings > Apps to
; uninstall it. Uninstalling leaves your projects, settings and backups.

Unicode true
!include "MUI2.nsh"

Name "Sloth Writer"
OutFile "${OUT}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Sloth Writer"
InstallDirRegKey HKCU "Software\Sloth Writer" "InstallDir"
BrandingText "Sloth Writer ${VERSION}"

!define MUI_ICON "..\..\icons\SlothWriter.ico"
!define MUI_UNICON "..\..\icons\SlothWriter.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\Sloth Writer.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Sloth Writer"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SlothWriter"

Section "Sloth Writer"
  SetOutPath "$INSTDIR"
  File "/oname=Sloth Writer.exe" "${EXE}"
  WriteUninstaller "$INSTDIR\Uninstall Sloth Writer.exe"
  CreateShortcut "$SMPROGRAMS\Sloth Writer.lnk" "$INSTDIR\Sloth Writer.exe"
  WriteRegStr HKCU "Software\Sloth Writer" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Sloth Writer"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Brad Salomons"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\Sloth Writer.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall Sloth Writer.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\Sloth Writer.exe"
  Delete "$INSTDIR\Uninstall Sloth Writer.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Sloth Writer.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "Software\Sloth Writer"
SectionEnd
