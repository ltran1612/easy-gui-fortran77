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

**`corpus/precision` pins the arithmetic itself.** Its expected values are IEEE
754 facts worked out away from this toolchain, read as bit patterns through
`EQUIVALENCE` so no `WRITE` formatting sits between the arithmetic and the
assertion. It covers single-precision storage, the widened-single trap in a
`DOUBLE PRECISION` assignment, denormals surviving (a zero there means something
turned on flush-to-zero), unreassociated accumulation, integer truncation and
mixed-mode evaluation. `tests/precision.rs` then asserts the two directions that
pinning alone leaves open: `-O0`, `-O1` and `-O2` must agree to the digit, and
turning on `default_real8` must *disagree* — the second is what stops the first
from passing for reasons unrelated to arithmetic.

## Things that look wrong and are not

- **`-ffixed-line-length-72` by default, not `none`.** Card-image code keeps
  sequence numbers in columns 73–80; widening turns those into syntax errors.
- **The window icon is set twice.** Windows takes it from the executable's
  resources (`ef-gui/build.rs`); everywhere else it comes from
  `ViewportBuilder::with_icon` at startup.
- **The pause shim is Windows-only, and that is not an oversight.** It stops a
  double-clicked console window vanishing (`assets/pause-shim.f90`, reached by
  `-Wl,--wrap=exit`) and is on by default. It cannot be shared with Linux: the
  constructor mechanism that would do it there does not fire on MinGW, and the
  wrap that works on MinGW does not fire on Linux. Opposite mechanisms, so the
  option is simply inert off Windows — `wants_pause_shim` gates on the
  toolchain's exe suffix, not the host.
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
