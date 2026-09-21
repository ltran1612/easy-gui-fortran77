# Architecture

For someone about to change this code. `README.md` says what the product is;
this says how it is put together and why it is put together that way.

## The rule everything else follows

**The application never modifies the user's source files.** Not a policy — a
structure. One module touches the filesystem, every write asserts where it
lands, CI fails the build if another module tries, and the test suite hashes the
whole corpus before and after every build and asserts it is byte-identical.

It is also why there is no editor in the UI, and why almost every design
question below resolves the same way: copy, never touch.

## Crates

| | Lines | What it is |
|---|---:|---|
| `ef-core` | ~7,800 | All logic. No GUI dependency, so the whole pipeline is testable headlessly. |
| `ef-gui` | ~2,100 | The window (`eframe`/`egui`). Holds no logic, and **may not touch the filesystem or spawn processes** — CI enforces both. |
| `ef-cli` | ~400 | Headless driver, for CI and for diagnosing an installation over the phone. |
| `ef-testkit` | ~200 + tests | A fake compiler and a fake program, plus the Fortran corpus. |
| `xtask` | — | Repository chores: hygiene, the bundled toolchain, packaging, icons. |

`ef-core` is the only crate worth learning first. The others are shells around it.

## How a build flows

`ef-gui` never compiles anything itself. It sends a `Program` to `ef-core` on a
worker thread and receives `BuildEvent`s back through a channel.

```
App::start_build          ef-gui/src/app.rs      spawns a thread, keeps drawing
  build::build            build/mod.rs           the state machine
    Preflight             ─ sources present, toolchain manifest verified
    Staging               build/stage.rs         copy sources into the work tree
                          build/sourcefmt.rs     refuse what is not Fortran text
                          build/objfmt.rs        refuse a library we cannot link
    Compiling             build/args.rs          one argv per file, one process each
    Linking               build/args.rs          objects, then libraries, then flags
  BuildOutcome            ─ exe path, diagnostics, or a typed FileProblem
App::save_program         fs_guard.rs            copy the built program out
```

Two decisions in there are load-bearing:

**Each source is compiled separately.** A single multi-file `gfortran` command
stops at the first file that fails, so the user would see errors in one file and
nothing about the other six. One process per file collects every error at once.

**Staging is what makes the promise keepable.** Sources are copied into the work
tree under normalised lowercase ASCII names. That simultaneously fixes the `.FOR`
preprocessor trap, keeps non-ASCII paths out of the compiler's argv, and means
the compiler only ever sees our copy.

## Where the compiler comes from

It is never in git. `toolchain/<target>.toml` pins every conda-forge package by
SHA-256; `cargo xtask fetch-toolchain` downloads, verifies, extracts, prunes to a
keep-list, and writes an integrity manifest the application embeds. CI builds it
and publishes it as a release artefact.

`toolchain/README.md` has the detail, including why the prune is a keep-list and
which three things must never be dropped from it.

**The version is pinned in four places and they are checked against each other.**
`gcc_version` in the recipe, the package filenames it names, `version` in the
bundle descriptor, and the GCC version that appears as a directory component in
the descriptor's `-B` paths. `check_version_is_pinned` in `xtask/src/fetch.rs`
fails the fetch if any of them disagree, and asks the compiler itself when the
host can run it. A bump that updates three of the four is the realistic mistake,
and two of its outcomes are silent.

**A damaged bundle is never replaced by a system compiler.** `discover` falls
through to `gfortran` on PATH only when no bundle was shipped at all; a bundle
that is present and will not load is reported as a damaged installation. This
matters more than it looks: Windows machines carry a gfortran on PATH for
reasons unrelated to us — Strawberry Perl, MSYS2, Anaconda — and quietly
compiling with one would hand the user a program built by a compiler nobody
pinned, with different numerics, and nothing on screen to say so.

## What CI enforces

`cargo xtask check-hygiene` turns the structural rules into build failures:

- only `fs_guard.rs` may create, write, rename or delete files;
- never spawn a child through a shell;
- `ef-gui` may not use `std::fs` or `std::process`;
- no binaries committed, nothing tracked over 1 MB;
- the committed toolchain manifest stays a placeholder;
- the committed icons still match `logo.png`;
- every translation key named in the code exists in **both** catalogs.

That last one matters more than it looks: `i18n::lookup` falls back to English
when a Vietnamese key is missing, so the failure is silent and the user simply
sees the wrong language.

One module is exempt from one rule, and the exemption is the point: `fs_guard.rs`
from the filesystem rule. The place that owns a hazard is the place allowed to
name it.

## Embedded and generated things, and what pins each

Several assets live inside the binary or are derived from something else. Each
one has a source of truth and a check that fails when they disagree. Adding a new
asset means adding a row here and the check that earns it — the shim sat
unguarded for a release because it was added without one.

| Asset | Source of truth | What pins it |
|---|---|---|
| `assets/i18n/{vi,en}.toml` | each other | key parity test, plus `check-hygiene` over every `tr!` site |
| `assets/help/{vi,en}/*.md` | each other | topic-id parity test |
| `assets/pause-shim.f90` | itself (Fortran) | `tests/shim.rs` compiles it and checks the symbols the link flag needs |
| `assets/toolchain-manifest.txt` | the fetched bundle | `check-hygiene` keeps the committed copy a placeholder; `ef-cli doctor` verifies the real one |
| `packaging/windows/icon.ico`, `assets/icon-128.png` | `logo.png` | `check-hygiene` regenerates and byte-compares |
| `examples/` | `examples/DOC-TRUOC.txt` | `tests/examples.rs` builds and runs each, and checks the guide names them |
| `ef-testkit/corpus/` | each case's `expect.toml` | `tests/corpus.rs` |

## Tests

| Tier | Needs | Where |
|---|---|---|
| 0 | nothing | unit tests beside the code |
| 1 | a fake compiler | `ef-testkit/tests/pipeline.rs` |
| 2 | real gfortran | `corpus.rs`, `libraries.rs`, `examples.rs` |

Tier 2 skips when no compiler is installed; `EF77_REQUIRE_TOOLCHAIN=1` turns that
skip into a failure, and CI sets it so the corpus can never be silently skipped.

Wrapped around every corpus case is the one assertion the product exists for: the
source tree is byte-identical before and after.

**Arithmetic is done the old compiler's way by default, as nearly as gfortran
can**: `-mfpmath=387` at `-O0`. Microsoft FORTRAN 3.30 worked each expression
out at extended precision on the x87 unit and rounded when it stored a variable.
Measured against a model of that, three candidates:

| | keeps `(A + B) − A`, A = 1.0E7, B = 0.3 | exact comparisons of stored values matching it |
|---|---|---|
| plain 32-bit | no — `0.0` | 1600 / 1600 |
| `REAL` widened to 80 bits (`-freal-4-real-10`, 0.1.14) | yes | 1527 / 1600 |
| **x87 at `-O0`** | **yes** | **1600 / 1600** |

Widening `REAL` stores more than the old machine did, so two values that used to
round to the same number can differ in the 19th digit and send a comparison the
other way. `-mfpmath=387 -ffloat-store` looks like the fix for x87's
optimisation-dependence and is not: it also rounds the compiler's hidden
temporaries, so it loses the `0.3` too. `-O0` is what works — every variable is
written back after every statement, exactly as a 1985 compiler did. At `-O1` the
imitation fails twice over: values stay in registers across statements, and a
formula whose inputs the compiler can see is worked out while compiling, in 32
bits. `REAL` stays 4 bytes throughout, so no storage hazard applies and a
library built elsewhere still gets the values it expects.

**It is not the old compiler.** Checked against Microsoft FORTRAN 3.30 itself,
run under DOSBox, the old compiler was a hybrid that no gfortran setting copies:

| | Microsoft FORTRAN 3.30 | plain 32-bit | x87 at `-O0` |
|---|---|---|---|
| formula stored in a variable, `C = (A + B) - A` | extended, rounded on store | loses the 0.3 | **matches** |
| calculation compared directly, `IF (X*100.0 .LT. R)` | rounds each side to `REAL` first | **matches** | differs when the sides agree to the last digit: 48 / 99 `.LT.`, 89 / 99 `.EQ.` |
| call partway through a formula, `(A + B + SIN(Z)) - A` | keeps extended | loses the 0.3 | loses the 0.3: the value so far is stored at 32 bits before the call |
| formula of literals only, `(1.0E7 + 0.3) - 1.0E7` | keeps the 0.3 | `0.0` | `0.0`: folded while compiling |
| `INT(X*100.0)`, or assignment to `INTEGER` | truncates the extended value | differs, 48 / 99 | differs, 48 / 99 |

Two more consequences of moving the arithmetic, neither visible in `REAL`
alone. `DOUBLE PRECISION` moves to the x87 as well, so its intermediates are
extended too — as they were on the old machine, and unlike plain 64-bit. And a
`REAL` is copied through the x87, so integer or text data kept in one by
`EQUIVALENCE` can change on the way: a bit pattern that is a signalling NaN comes
out quietened, where the old compiler copied bytes.

It stays the default because legacy engineering code mostly works a formula out,
stores it, and compares stored variables — the rows this gets right. A
calculation compared directly is safe in every setting once it is stored in a
variable first.

`-O0` costs speed: about 9× on a 400 × 400 matrix product against `-O1`, which
at the sizes these programs run is a fraction of a millisecond. It also switches
off `-ffrontend-optimize`, and with it the skipping of the right-hand side of
`.AND.`/`.OR.` once the left has decided — so `IF (J .NE. 0 .AND. K/J .GT. 1)`
divides by zero and crashes. Every `-O0` build therefore asks for
`-ffrontend-optimize` by name; it restores the skipping and leaves the arithmetic
as it was.

One predicate decides all of it: `BuildOptions::old_compiler_arithmetic` — the
option is on *and* the compiler has an x87. The `-mfpmath=387`, the level from
`BuildOptions::effective_opt_level`, and whether the window greys the level out
are all read from it, so a compiler without an x87 builds plain arithmetic at
the level asked for, not at a pointless `-O0`.

**`tests/precision.rs` pins what it gets right**, each check chosen so it fails
if the mode stops working: `REAL` stays 4 bytes (0.1 read back through
`EQUIVALENCE` as `0x3DCCCCCD`); the `0.3` survives where plain 32-bit loses it;
asking for `-O1` or `-O2` changes nothing, checked for the right digits and not
only for agreement; two exact comparisons of stored values go the way the old
compiler sent them rather than the way 80 bits did; and the `.AND.` guard above
runs cleanly. Every program there must exit cleanly, so a build that produced
nothing cannot pass by comparing nothing with nothing. Removing the `-O0`
enforcement fails two of the checks; dropping `-ffrontend-optimize` fails the
guard.

`corpus/precision` pins IEEE facts under the shipped default: `REAL` is 4 bytes,
single-precision stores, unreassociated accumulation, integer division and
`MOD`, mixed mode. Two of its checks no longer test what they were written for
under x87. The denormal check cannot see flush-to-zero, which only ever touches
SSE arithmetic. The widened-single check (`D = 1.0/3.0` gains no digits) passes
because the compiler folds the literal division in 32 bits; the same division of
two variables at run time does gain them under x87. Plain arithmetic, the
option turned off, is not run through the corpus.

## Things that look wrong and are not

- **`-ffixed-line-length-72` by default, not `none`.** Card-image code keeps
  sequence numbers in columns 73–80; widening turns those into syntax errors.
- **The window icon is set twice.** Windows takes it from the executable's
  resources (`ef-gui/build.rs`); everywhere else it comes from
  `ViewportBuilder::with_icon` at startup.
- **The pause shim is Windows-only, and that is not an oversight.** It stops a
  double-clicked console window vanishing (`assets/pause-shim.f90`, reached by
  `-Wl,--wrap=exit`) and is on by default. Its one untestable step -- whether
  `GetConsoleProcessList` returns 1 for a double-clicked program -- was checked
  by hand on Windows and does. Nothing here can check it: wine runs every test
  around it and none of them exercise that. It cannot be shared with Linux: the
  constructor mechanism that would do it there does not fire on MinGW, and the
  wrap that works on MinGW does not fire on Linux. Opposite mechanisms, so the
  option is simply inert off Windows — `wants_pause_shim` gates on the
  toolchain's exe suffix, not the host.
- **The optimisation level is ignored by default.** Not a bug:
  `BuildOptions::effective_opt_level` forces `-O0` while arithmetic is done the
  old compiler's way, because optimisation changes where values get rounded. The
  saved level is kept and applies again if that option is turned off — or if the
  compiler has no x87 to do it on. The window greys the setting out and says why.
- **`-ffrontend-optimize` on a build that is not optimised.** `-O0` switches it
  off, and it is what makes `.AND.`/`.OR.` skip their right-hand side; old code
  guards divisions that way. It changes no arithmetic.
- **`objfmt` parses OMF rather than sniffing its first byte.** `0xF0` is an OMF
  library header and also `đ` in the Vietnamese codepage the user's comments are
  written in.

## Where to look

| Question | File |
|---|---|
| What flags does a build use, and why? | `build/args.rs` |
| Why was my file refused? | `build/sourcefmt.rs`, `build/objfmt.rs` |
| How is a toolchain found and trusted? | `toolchain/mod.rs`, `toolchain/manifest.rs` |
| Where does anything get written? | `fs_guard.rs` — everywhere, or nowhere |
| What does the user see? | `assets/i18n/*.toml`, `assets/help/**` |
| How is a release built? | `.github/workflows/release.yml`, `xtask/src/package.rs` |
