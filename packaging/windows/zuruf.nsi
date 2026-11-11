Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef STAGE
  !error "STAGE is required"
!endif
!ifndef OUTPUT
  !error "OUTPUT is required"
!endif

Name "Zuruf"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Zuruf"
InstallDirRegKey HKCU "Software\Zuruf" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 32
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1031 "ProductName" "Zuruf"
VIAddVersionKey /LANG=1031 "FileDescription" "Zuruf Installer"
VIAddVersionKey /LANG=1031 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1031 "LegalCopyright" "Copyright 2026 Zuruf contributors"
!define MUI_ICON "..\..\assets\icon\zuruf.ico"
!define MUI_UNICON "..\..\assets\icon\zuruf.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\Zuruf.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Zuruf öffnen"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "English"

Section "Zuruf" Main
  SetShellVarContext current
  IfFileExists "$INSTDIR\bin\zuruf.exe" 0 +3
    nsExec::ExecToLog /TIMEOUT=20000 '"$INSTDIR\bin\zuruf.exe" stop'
    Sleep 1500
  SetOutPath "$INSTDIR"
  File /r "${STAGE}\*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Zuruf"
  CreateShortcut "$SMPROGRAMS\Zuruf\Zuruf.lnk" "$INSTDIR\Zuruf.exe"
  CreateShortcut "$SMPROGRAMS\Zuruf\Deinstallieren.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Zuruf" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "DisplayName" "Zuruf"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "Publisher" "Zuruf contributors"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "URLInfoAbout" "https://github.com/LordOfMim/zuruf"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "DisplayIcon" "$INSTDIR\Zuruf.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  IfFileExists "$INSTDIR\bin\zuruf.exe" 0 +3
    nsExec::ExecToLog /TIMEOUT=20000 '"$INSTDIR\bin\zuruf.exe" stop'
    Sleep 1500
  Delete "$SMPROGRAMS\Zuruf\Zuruf.lnk"
  Delete "$SMPROGRAMS\Zuruf\Deinstallieren.lnk"
  RMDir "$SMPROGRAMS\Zuruf"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zuruf"
  DeleteRegKey HKCU "Software\Zuruf"
  ; User data lives separately in LOCALAPPDATA\Zuruf and is deliberately preserved.
  RMDir /r "$INSTDIR"
SectionEnd
