#!/usr/bin/env bash
# Build the Windows installer on Linux and exercise it under wine.
#
# CI builds the real thing on windows-latest. This exists so the installer can be
# changed without waiting for a push to find out it is broken, and it checks the
# parts that are easy to get wrong and invisible until someone runs it: whether
# it installs without asking for administrator rights, whether the shortcuts and
# the uninstall entry appear, whether the bundled compiler still works from the
# installed location, and whether uninstalling leaves the user's saved programs
# alone.
#
# The application binary is a placeholder unless one has been cross-built: this
# machine has no Windows Rust target. What is validated is the installer, not the
# application inside it.
set -euo pipefail

command -v makensis >/dev/null || { echo "makensis is not installed (dnf install mingw32-nsis)"; exit 1; }
command -v wine     >/dev/null || { echo "wine is not installed"; exit 1; }

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
version="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
staged="$repo/target/dist/EasyFortran77-$version-windows-x86_64"
setup="$repo/target/dist/EasyFortran77-$version-Setup.exe"
export WINEPREFIX="${EF77_WINEPREFIX:-$repo/target/wineprefix}"
export WINEDEBUG=-all

[ -d "$staged" ] || { echo "no staged package; run: cargo xtask package --target windows-x86_64"; exit 1; }

echo "== building the installer =="
makensis -DSRC="$staged" -DVERSION="$version" \
  -DICON="$PWD/packaging/windows/icon.ico" \
  -DOUT="$setup" packaging/windows/installer.nsi | tail -3
ls -l "$setup" | awk '{printf "installer: %.1f MB\n", $5/1e6}'

user_dir="$WINEPREFIX/drive_c/users/$USER"
installed="$user_dir/AppData/Local/Programs/Easy Fortran 77"
config="$user_dir/AppData/Roaming/Easy Fortran 77/config"

# Plant something that must survive uninstalling: the user's list of programs.
mkdir -p "$config"
printf 'schema_version = 1\n\n[[program]]\nname = "Tinh dam be tong"\n' > "$config/programs.toml"

echo
echo "== installing (silently, and with no elevation) =="
wine "$setup" /S
[ -d "$installed" ] || { echo "FAIL: nothing installed at $installed"; exit 1; }
du -sh "$installed" | awk '{print "installed:", $1}'

echo
echo "== what it left behind =="
find "$WINEPREFIX/drive_c" -path '*Start Menu*' -name '*.lnk' 2>/dev/null | sed 's|.*Programs/|  shortcut: |'
wine reg query 'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\EasyFortran77' 2>/dev/null \
  | grep -E 'DisplayName|DisplayVersion' | sed 's/^\s*/  /'

echo
echo "== the examples came with it =="
for f in examples/DOC-TRUOC.txt examples/01-CO-BAN.FOR examples/03-CHUONG-TRINH-CON/THAMSO.INC; do
  [ -f "$installed/$f" ] && echo "  $f" || { echo "FAIL: $f was not installed"; exit 1; }
done

echo
echo "== the installed compiler still works =="
t="$(mktemp -d)"; printf '      PROGRAM T\n      WRITE (*,*) 6*7\n      END\n' > "$t/t.f"
tr=x86_64-w64-mingw32; v=16.2.0
wine "$installed/toolchain/Library/bin/$tr-gfortran.exe" \
  --sysroot="$installed/toolchain/Library/$tr/sysroot" \
  -B"$installed/toolchain/Library/bin/" -B"$installed/toolchain/Library/$tr/bin/" \
  -B"$installed/toolchain/Library/libexec/gcc/$tr/$v/" \
  -B"$installed/toolchain/Library/lib/gcc/$tr/$v/" -B"$installed/toolchain/lib/gcc/$tr/$v/" \
  -fno-use-linker-plugin -std=legacy -static -o "$t/t.exe" "$t/t.f"
wine "$t/t.exe"

echo
echo "== uninstalling =="
wine "$installed/Uninstall.exe" /S || true
sleep 2
fail=0
[ -d "$installed" ] && { echo "FAIL: install directory survived"; fail=1; } || echo "  install directory removed"
[ "$(find "$WINEPREFIX/drive_c" -path '*Start Menu*' -name '*Fortran*' 2>/dev/null | wc -l)" -eq 0 ] \
  && echo "  shortcuts removed" || { echo "FAIL: shortcuts survived"; fail=1; }
# The one thing uninstalling must NOT take with it.
[ -f "$config/programs.toml" ] && echo "  the user's saved programs kept" || { echo "FAIL: the user's saved programs were deleted"; fail=1; }
rm -rf "$t"
exit $fail
