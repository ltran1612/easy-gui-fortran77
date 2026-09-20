; Easy Fortran 77 — Windows installer.
;
; Built by CI with makensis over a staged package directory, i.e. the output of
; `cargo xtask package --target windows-x86_64`.
;
;   makensis -DSRC=<staged dir> -DVERSION=<x.y.z> -DICON=<icon.ico> \
;            -DOUT=<file.exe> installer.nsi
;
; Two decisions worth stating, because both are deliberate:
;
;   * Per-user install, so there is no UAC prompt. An elevation dialog on an
;     unsigned installer is where a non-technical user stops, and that costs more
;     than the marginal benefit of an admin-only install directory. The toolchain
;     integrity manifest recovers most of that anyway.
;
;   * The interface is English (NSIS's own), but every string this file supplies
;     is Vietnamese first. The person installing this is Vietnamese-first, and
;     the application itself is fully bilingual.

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef SRC
  !error "SRC must be defined: the staged package directory"
!endif
; Absolute, and required rather than defaulted: `${__FILEDIR__}` resolves
; differently depending on where makensis was invoked from.
!ifndef ICON
  !error "ICON must be defined: the path to icon.ico (cargo xtask gen-icons)"
!endif
!ifndef OUT
  !define OUT "EasyFortran77-Setup.exe"
!endif

!define APPNAME "Easy Fortran 77"
!define COMPANY "Easy Fortran 77"
!define REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\EasyFortran77"

Name "${APPNAME} ${VERSION}"
OutFile "${OUT}"
; Per-user: no UAC prompt, and no admin rights needed.
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Easy Fortran 77"
InstallDirRegKey HKCU "Software\EasyFortran77" "InstallDir"
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUnInstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APPNAME}"
VIAddVersionKey "FileDescription" "${APPNAME} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT OR Apache-2.0; bundled GNU toolchain under GPLv3"

; The icon on Setup.exe is the first thing the user sees, before anything is
; installed -- it is on the file they download. Same artwork as the application
; and the uninstaller, generated from logo.png by `cargo xtask gen-icons`.
;
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "${APPNAME}"
!define MUI_WELCOMEPAGE_TEXT "Chương trình này giúp bạn biên dịch các chương trình Fortran 77 mà không cần dùng dòng lệnh.$\r$\n$\r$\nThis installs ${APPNAME}, which compiles Fortran 77 programs without needing a command line.$\r$\n$\r$\nNó sẽ được cài vào thư mục cá nhân của bạn, nên không cần quyền quản trị.$\r$\nIt installs into your own user folder, so no administrator rights are needed."

; The GPL applies to the bundled compiler, so the text travels with it.
!insertmacro MUI_PAGE_WELCOME
!define MUI_LICENSEPAGE_TEXT_TOP "Trình biên dịch đi kèm dùng giấy phép GPL phiên bản 3."
!define MUI_LICENSEPAGE_BUTTON "Tiếp tục / Continue"
!define MUI_LICENSEPAGE_TEXT_BOTTOM "Chương trình bạn tự biên dịch KHÔNG bị ràng buộc bởi giấy phép này.$\r$\nPrograms you compile yourself are NOT covered by it."
!insertmacro MUI_PAGE_LICENSE "${SRC}\LICENSES\gpl-3.0.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\easy-fortran-77.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Mở ${APPNAME} ngay bây giờ / Open ${APPNAME} now"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  ; The whole staged package: the application, the toolchain beside it, and the
  ; licences. The application looks for the compiler at <exe>\..\toolchain.
  File /r "${SRC}\*.*"

  WriteRegStr HKCU "Software\EasyFortran77" "InstallDir" "$INSTDIR"
  CreateDirectory "$SMPROGRAMS\${APPNAME}"
  CreateShortCut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\easy-fortran-77.exe"
  CreateShortCut "$SMPROGRAMS\${APPNAME}\Gỡ cài đặt - Uninstall.lnk" "$INSTDIR\Uninstall.exe"

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegStr   HKCU "${REGKEY}" "DisplayName"     "${APPNAME}"
  WriteRegStr   HKCU "${REGKEY}" "DisplayVersion"  "${VERSION}"
  WriteRegStr   HKCU "${REGKEY}" "Publisher"       "${COMPANY}"
  WriteRegStr   HKCU "${REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr   HKCU "${REGKEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${REGKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${REGKEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${REGKEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  ; Remove only what was installed. The user's own Fortran files are never here,
  ; and the user's saved programs live in AppData, which is left alone: uninstalling
  ; should not throw away the user's list of programs.
  Delete "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\${APPNAME}\Gỡ cài đặt - Uninstall.lnk"
  RMDir  "$SMPROGRAMS\${APPNAME}"

  Delete "$INSTDIR\easy-fortran-77.exe"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR\toolchain"
  RMDir /r "$INSTDIR\LICENSES"
  ; The examples ship with the application, so they go with it. The RMDir below
  ; is deliberately non-recursive -- it removes the directory only when nothing
  ; is left in it -- so anything installed here has to be named above or the
  ; install directory survives the uninstall.
  RMDir /r "$INSTDIR\examples"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${REGKEY}"
  DeleteRegKey HKCU "Software\EasyFortran77"
SectionEnd
