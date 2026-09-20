# Bundled toolchains

The application ships its own Fortran compiler so nobody ever has to install one.
Each platform has a *recipe* here naming the exact packages, pinned by SHA-256,
and a `bundle.toml` template telling the application how to drive the result.

| File | What it is |
|---|---|
| `linux-x86_64.toml` | pinned conda-forge packages, and the prune rules |
| `linux-x86_64.bundle.toml` | the `bundle.toml` shipped inside that bundle |

## Verifying what was built

The fetch task writes `crates/ef-gui/assets/toolchain-manifest.txt`, a SHA-256 of
every file in the bundle, and the application embeds it. At startup it re-hashes
the bundle in the background and refuses to build with a compiler that does not
match — distinguishing a **missing** file (almost always antivirus, and
recoverable) from a **changed** one (not, and not).

The manifest is embedded rather than shipped inside the bundle deliberately: one
sitting next to the compiler could be rewritten by whatever rewrote the compiler.

The committed copy is a placeholder. Running the fetch task overwrites it, and
that generated version is **not** committed — it belongs to one specific fetched
bundle, and a release regenerates it during packaging.

## Exercising the Windows bundle from Linux

```sh
toolchain/verify-under-wine.sh
```

The Windows recipe is written and pruned on a machine that cannot run its
binaries. Without this there is nothing between "the files look right" and
"Windows CI said so", which is a long way to push a mistake.

The script gives the fetched bundle a `launcher = "wine"` and a `WINEPREFIX`
through its `bundle.toml`, then probes capabilities and runs the whole corpus.
All fifteen cases build and run, static linking and stripping included. It uses
its own wine prefix under `target/`, not your `~/.wine`.

A shipped bundle carries none of that: it runs natively, so the launcher is
appended to the fetched copy rather than put in the template.

**wine is not Windows.** A pass here is evidence, not proof, and the
`windows-latest` CI job is still the gate. It earns its place anyway — it found
two real bugs the day it was written:

- the build looked for `program` where MinGW's linker writes `program.exe`,
  because the suffix came from `std::env::consts::EXE_SUFFIX` — the *host's* —
  rather than from the bundle's target;
- the launcher was resolved against the child's PATH, which is scrubbed down to
  the bundle's own directories, so a bare `wine` could never be found there and
  every probe failed for a reason that looked nothing like the cause.

## Corresponding Source (GPLv3 §6)

```sh
cargo xtask fetch-sources --list     # what we are obliged to publish, and from where
cargo xtask fetch-sources            # download it and write SOURCES.md
```

Shipping GCC and binutils binaries obliges us to offer their source from the same
place, for as long as we distribute them. conda-forge does not host tarballs next
to its packages, so citing conda-forge would not discharge it — we mirror.

Section 1 counts the build scripts as Corresponding Source, so the feedstock
recipes and their patches are listed too, not as a courtesy: conda-forge patches
GCC, so the upstream tarball alone is not sufficient. The checksums are
conda-forge's own, taken from the recipes that built the binaries we ship.

## Building a bundle

```sh
cargo xtask fetch-toolchain                    # --target linux-x86_64 by default
cargo xtask fetch-toolchain --offline          # use the cache, never the network
```

It downloads each package, **verifies its SHA-256 against the recipe**, extracts
it, prunes to the keep-list, drops in `bundle.toml`, and writes the integrity
manifest the application embeds. Archives are cached under
`target/toolchain-cache/`, so a re-prune costs nothing.

Then prove it, which the command tells you to do and which is not optional:

```sh
EF77_REQUIRE_TOOLCHAIN=1 EF77_TOOLCHAIN_BUNDLE=$PWD/target/toolchain/linux-x86_64 \
  cargo test -p ef-testkit --test corpus
```

## Why conda-forge on Linux

A Linux `gfortran` normally links against the *host's* glibc: it needs
`crt1.o`/`crti.o`/`crtn.o` and, for a static link, `libc.a` — all from a `-devel`
package most desktops do not have. Copying a GCC tree between distros therefore
does not work on its own.

conda-forge's toolchain solves both halves:

- It is **relocatable by construction.** GCC is configured `--with-sysroot` *under*
  `--prefix`, which is the condition for `TARGET_SYSTEM_ROOT_RELOCATABLE`, and the
  driver recomputes its own prefixes from `argv[0]`. Move the tree, it still works.
- `sysroot_linux-64` includes **`glibc-static`**, so produced programs can be
  linked fully static and run on any x86-64 Linux whatever its glibc — the same
  guarantee the Windows bundle gives.

**glibc 2.17** is the floor: every distro since mid-2014 has at least that. It is
the same baseline as manylinux2014.

Ruled out, with reasons: musl (no maintained prebuilt gfortran — musl.cc has been
frozen since 2021 and AmanoTeam's builds are C/C++ only); Flatpak (no gfortran in
the SDK, and the only extension died in 2018); Julia's Yggdrasil (~1 GB per shard,
fixed mount point, and every shard is musl-*hosted* so it cannot give a
Windows-hosted compiler); Spack build caches (padded-prefix relocation is at odds
with installing into a user's home directory).

## Validation performed (2026-09-19)

Verified by actually doing it, not by reading the recipes:

1. All packages downloaded and **SHA-256 verified**, extracted with no conda
   installed and no prefix replacement.
2. `-print-file-name=crt1.o` resolves **inside the bundle's sysroot**, not
   `/usr/lib64`.
3. A static link **succeeds on a host that has no `libc.a`** — which it could only
   do using the bundled sysroot.
4. Output is `statically linked`, `ldd` says `not a dynamic executable`.
5. The **whole tree was moved** to a deeply nested path and rebuilt unchanged.
6. The produced program runs on **Debian 12** (glibc 2.36) with no gcc and no
   gfortran.
7. The **toolchain itself** compiles and links on Debian 12 with `PATH` containing
   *only* the bundle, so no host fallback was possible.
8. All 15 Fortran corpus cases pass through the bundle, identical to the system
   compiler, and the capability probe reports `-static: yes` (the system gfortran
   on the development box reports `no`).
9. The Windows bundle builds and runs the whole corpus under wine (see above).
10. Re-verified after pruning to 118 MB: all 15 corpus cases, every capability
   still `yes`, and a clean Debian container compiling both a `.f` and a
   preprocessed `.F`, statically linked and stripped.

Step 7 is the one that found a real gap: `ld` lives in a *separate*
`ld_impl_linux-64` package, and without it `collect2` fails with `cannot find 'ld'`.
On a host with `/usr/bin/ld` that failure is invisible, because the host's linker
gets used instead — exactly the silent fallback this design exists to prevent.

## The prune: 585 MB to 118 MB

The keep-list lives in `linux-x86_64.toml`. It is a keep-list rather than a
drop-list so that if a future GCC moves something we need, the corpus fails loudly
instead of a smaller bundle silently shipping that cannot link.

The four things that account for nearly all of it:

| Dropped | Size | Why it is safe |
|---|---|---|
| `locale-archive.tmpl` | 102 MB | glibc locale data; the compiler runs under `LC_ALL=C` |
| `cc1`, `cc1obj`, `cc1objplus`, `lto1` | 182 MB | C/ObjC front ends and LTO. **f951 has an integrated preprocessor**, so even `.F` sources compile without `cc1` — verified in a container with nothing else installed |
| `libstdc++.so.6` | 23 MB | nothing in the bundle links it; `ldd` on the driver, `f951`, `collect2`, `as` and `ld` shows only host libc/libm plus the bundle's own libzstd |
| sanitizers, `usr/include`, docs, man, info | ~60 MB | no C compiler ships, so C headers are unusable; sanitizers are a developer tool |

Three things the prune must **not** drop, each found by the corpus rather than by
reading:

- **Directory symlinks.** The sysroot has `usr/lib -> lib64`, and GCC resolves
  `crt1.o` through `usr/lib/../lib/`. Walking files alone loses these and the
  failure appears only at link time, as `cannot find crt1.o`.
- **`libatomic_asneeded`**, which the specs link unconditionally.
- **`ld_impl_linux-64`**, a package separate from binutils. Without it `collect2`
  reports `cannot find 'ld'` — but only on a machine that has no `/usr/bin/ld`,
  so a development box hides it entirely.

One flag earns its place in `bundle.toml`: **`-fno-use-linker-plugin`**, on both
the compile and link lines. GCC's specs reference the LTO plugin and the driver
resolves that eagerly — it fails even for `-fsyntax-only`. We never pass `-flto`,
so turning the plugin off is better than carrying 2.7 MB of LTO machinery.

Also visible in those specs, and worth knowing: the conda build-prefix placeholder
survives as `%{!static:-rpath /home/conda/feedstock_root/...}`. It is guarded by
`%{!static:...}` and we always link static, so it never fires — which is one more
reason the static link is not merely a portability nicety.

## The Windows prune

Same shape as the Linux one, with the mingw sysroot's import libraries named file
by file rather than kept wholesale. That directory is 82 MB, nearly all of it
imports for Windows API surfaces a Fortran program never touches — OneCore,
WindowsApp, nanoserver.

The list is not a guess. `ld -t` reports every file the linker opens, so running
that across all corpus programs and four link variants (static, dynamic,
stripped, large-stack) gives the authoritative set. It comes to exactly ten
files:

    crt2.o  default-manifest.o  libadvapi32.a  libkernel32.a  libm.a
    libmingw32.a  libmingwex.a  libmsvcrt.a  libshell32.a  libuser32.a

The rest of the keep-list is margin: everything the spec file names on a
conditional path, plus the classic Win32 import libraries a 1980s program might
name itself through the advanced "extra flags" box.

| | before | after |
|---|---|---|
| bundle | 176 MB | 113 MB |
| zip | 44.5 MB | 38.7 MB |
| installer | 27.5 MB | 25.4 MB |
| installed | ~177 MB | ~113 MB |

The installer barely moves, because import libraries are mostly repeated symbol
tables and compress away to almost nothing. What the user gains is 64 MB of disk
after installing.

Verified with the whole corpus under wine, with capabilities unchanged
(`-static` included), and with a dynamic link still producing a program that
runs — which needs `libgfortran-5.dll` beside it, and is exactly why the default
link is static.

## Not yet done

- **Real Windows.** Everything above is wine or structural inspection. The
  `windows-latest` CI job is the gate, and a local VM would additionally cover
  what neither reaches: SmartScreen on an unsigned installer, Defender
  quarantining `f951.exe`, and a profile path with Vietnamese diacritics.
- **One pipeline or two.** The Windows bundle now comes from conda-forge in the
  same format as the Linux one, so the WinLibs alternative is no longer needed —
  but nothing has been deleted, and that choice could still be revisited if the
  conda win-64 packages ever prove awkward.
- **Hosting the Corresponding Source.** `fetch-sources` gathers it and the
  release workflow publishes it, but that only discharges GPLv3 §6 for as long
  as the release stays up. Keep it up.
