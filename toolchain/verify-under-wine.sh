#!/usr/bin/env bash
# Exercise the Windows toolchain bundle from Linux, under wine.
#
# The Windows recipe is written and pruned on Linux, where its binaries cannot
# run, so without this there is nothing between "the files look right" and
# "Windows CI said so". wine is not Windows and a pass here is not proof — but it
# catches the things that would otherwise reach CI, and it found two real bugs
# the day it was written: the build looked for `program` where MinGW's linker
# writes `program.exe`, and the launcher was resolved against the child's
# scrubbed PATH, where it could never be found.
#
# Usage:  toolchain/verify-under-wine.sh
set -euo pipefail

command -v wine >/dev/null || { echo "wine is not installed"; exit 1; }

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle="$repo/target/toolchain/windows-x86_64"
prefix="${EF77_WINEPREFIX:-$repo/target/wineprefix}"

if [ ! -d "$bundle" ]; then
  echo "building the bundle first"
  (cd "$repo" && cargo xtask fetch-toolchain --target windows-x86_64)
fi

if [ ! -d "$prefix" ]; then
  echo "creating a wine prefix at $prefix (not your ~/.wine)"
  WINEPREFIX="$prefix" WINEDEBUG=-all wineboot -i >/dev/null 2>&1 || true
fi

# A shipped bundle runs natively and carries none of this. Appending it here
# rather than putting it in the template keeps the cross-testing arrangement out
# of what users receive.
if ! grep -q '^launcher' "$bundle/bundle.toml"; then
  cat >> "$bundle/bundle.toml" <<TOML

# Added by verify-under-wine.sh. Cross-testing from Linux only.
launcher = "wine"

[env]
# The child environment is scrubbed to an allowlist, so wine would otherwise
# lose its prefix and quietly build a fresh one in \$HOME.
WINEPREFIX = "$prefix"
WINEDEBUG = "-all"
TOML
fi

echo
echo "== capabilities, probed through wine =="
WINEPREFIX="$prefix" WINEDEBUG=-all \
  cargo run -q -p ef-cli --manifest-path "$repo/Cargo.toml" -- doctor \
  2>&1 | sed -n '/capabilities/,$p'

echo
echo "== the whole Fortran corpus, built and run through wine =="
cd "$repo"
EF77_REQUIRE_TOOLCHAIN=1 EF77_TOOLCHAIN_BUNDLE="$bundle" \
  WINEPREFIX="$prefix" WINEDEBUG=-all \
  cargo test -p ef-testkit --test corpus -- --nocapture every_corpus
