# Build options: what each one does, and why it starts where it does

Every option in **Advanced options** is recorded here: what it puts on the
compile line, which way it starts, the reason for that, and when it is worth
changing. `crates/ef-core/src/project.rs` holds the same reasons next to the
fields themselves; this file is the one place they are all together.

The defaults are chosen for one kind of program: engineering arithmetic written
for a DOS-era compiler, by someone who should never have to open this file. So
a default is only permissive where being strict would reject working code, and
only strict where being permissive would hand back a wrong number.

## The defaults at a glance

| Option | Default | Flag it adds |
|---|---|---|
| Dialect | Legacy | `-std=legacy` |
| Line length | 72 columns | `-ffixed-line-length-72` |
| DEC/Microsoft extensions | on | `-fdec` |
| Static local storage, zero-initialised | on | `-fno-automatic -finit-local-zero` |
| `D` lines as code | off | `-fd-lines-as-code` when on |
| 8-byte `REAL` | off | `-fdefault-real-8 -fdefault-double-8` when on |
| Old compiler's arithmetic | on | `-mfpmath=387`, and forces `-O0 -ffrontend-optimize` |
| Large local arrays | off | `-fmax-stack-var-size=0` when on |
| Stop on an array out of bounds | on | `-fcheck=bounds` |
| Wait for a key before the window closes | on | pause shim, `-Wl,--wrap=exit` (Windows) |
| Strip the saved program | off | `-s` at link time when on |
| Optimisation level | `-O1` | `-O0` / `-O1` / `-O2`, ignored while the old compiler's arithmetic is on |
| Preprocess | never | `-x f77-cpp-input` when on |
| Extra flags | empty | passed through verbatim |

## Dialect — Legacy

`-std=legacy` restores the five things Fortran 90 deleted and 1980s code is
full of: the arithmetic `IF`, `PAUSE`, `ASSIGN`, the assigned `GOTO`, and a
`REAL` loop variable. It also implies `-fallow-argument-mismatch`, which legacy
code needs constantly — passing a `REAL` array where a subroutine declares a
scalar, and so on, which was ordinary practice and is an error by default.

**Why the default:** the alternative, Standard, rejects working programs on
sight. Standard exists for someone who wants to know what in their code is not
Fortran 95; it is a checking tool, not a way to build.

## Line length — 72 columns

`-ffixed-line-length-72`. Fixed-form Fortran only ever reads columns 7 to 72.

**Why the default:** card-image code often carries sequence numbers in columns
73 to 80. Read as code, those numbers are syntax errors. The 132-column setting
is for code written for a compiler that allowed it, and *unlimited* should be a
last resort: it turns any stray text at the end of a line into code.

**When to change it:** a statement that looks complete but gets
"unterminated character constant" or "syntax error" at its end usually ran past
column 72.

## DEC/Microsoft extensions — on

`-fdec` is an umbrella for the VAX/DEC lineage that Microsoft FORTRAN, Lahey,
Watcom and Digital Visual Fortran all inherited: `STRUCTURE`/`RECORD`/`UNION`/
`MAP` with `P.FIELD` access, the size-specific integer intrinsics (`IIAND`,
`JIAND`), the degree trigonometry functions (`SIND`, `COSD`, `COTAN`),
`STATIC`/`AUTOMATIC`, `INCLUDE` as a statement, default widths in a `FORMAT`,
and text literals assigned to numbers.

**Why the default:** it only permits spellings. It changes no arithmetic and
rejects nothing that would otherwise compile, so leaving it on costs nothing
and saves a person hunting for which of eight sub-flags their code needs.

## Static local storage, zero-initialised — on

`-fno-automatic` gives every local variable the lifetime of the program, and
`-finit-local-zero` starts it at zero.

**Why the default:** DOS-era compilers laid out locals in static, zeroed
storage. Code that relies on a counter keeping its value between calls, or an
accumulator starting at zero without being set, is correct under that model and
silently wrong under modern stack allocation. This is the most common cause of
"it gives different numbers than it did in 1987".

**Cost of leaving it on:** the program cannot recurse, and a genuinely
uninitialised variable reads as zero instead of being caught. For code of this
era that trade is worth it.

## `D` lines as code — off

In fixed form, a `D` in column 1 marks a debugging line. It is a comment unless
the compiler is told otherwise; `-fd-lines-as-code` compiles it.

**Why the default:** those lines were switched off deliberately. Turning them
on adds print statements and checks to a program that is expected to be quiet.

## 8-byte `REAL` — off

`-fdefault-real-8` makes every `REAL` eight bytes. The app adds
`-fdefault-double-8` with it, because otherwise gfortran also promotes
`DOUBLE PRECISION` to sixteen bytes of software-emulated quad arithmetic — an
option named "use 8-byte REAL" has no business slowing down the part of a
program that was already careful about precision.

**Why the default is off:** it changes storage sizes, so unformatted files
written by an earlier build stop being readable, `EQUIVALENCE` overlays see
different bytes, and a pre-compiled library still expecting 4-byte values
receives 8-byte ones and returns nonsense with no error. It is a real answer to
a genuine precision problem, but it is not a free one.

## Old compiler's arithmetic — on

`-mfpmath=387`, and it forces `-O0` (plus `-ffrontend-optimize`, see below).
Arithmetic is done on the x87 unit at extended precision inside a statement and
rounded to 32 bits when a value is stored, which is how Microsoft FORTRAN 3.30
worked. `REAL` stays 4 bytes.

**Why the default:** on what these programs mostly do — work a formula out and
store it — it agrees with the old compiler where plain 32-bit arithmetic does
not. `(A + B) - A` with `A = 1.0E7` and `B = 0.3` gives 0.3, as it did on the
old machine, and 0.0 in plain 32-bit.

**Where it still differs from the old compiler** (measured against it, running
under DOSBox): a calculation compared directly in an `IF` without being stored
first; a formula that calls `SIN`, `EXP`, `ALOG`, `**` with a REAL power or a
user function partway through; a formula written entirely in literal numbers;
and `INT` of a calculation. `ARCHITECTURE.md` has the table, and the option's
doc comment in `project.rs` has the detail.

**When to turn it off:** to compare against an older build, to link a library
built elsewhere and have both sides round identically, or when a long
calculation is too slow — turning it off restores the optimisation level.

## Large local arrays — off

`-fmax-stack-var-size=0` moves local arrays into static memory.

**Why the default is off:** it is rarely what fixes anything. Modern gfortran
already places large fixed-size local arrays in static memory, and the static
storage default above reinforces that. This is the fallback for a program that
dies on entry with a stack overflow — exit code `0xC00000FD` on Windows, where
a thread gets 1 MB of stack.

## Stop on an array out of bounds — on

`-fcheck=bounds`.

**Why the default:** this is the one default that deliberately departs from
reproducing a 1980s compiler. Without it, `ARR(7)` on a three-element array
returns whatever sits next in memory: not a crash, not a NaN, just a plausible
number that flows into a result and is believed. For arithmetic someone builds
a bridge on, a program that stops and says "index 7 is outside 1 to 3" is worth
more than one that prints a confident wrong answer. It costs nothing at compile
time and little at run time at these sizes.

**When to turn it off:** old code that reads past an array on purpose — which
exists, and used to work.

## Wait for a key before the window closes — on

Links a small shim (`assets/pause-shim.f90`, reached by `-Wl,--wrap=exit`) that
waits for a key before the program exits. Windows only: the mechanism does not
exist on Linux, and the option is inert there.

**Why the default:** double-clicking the program in Explorer is what someone
who has never used a terminal will do, and without this that is a black window
that flashes and disappears, results unread. The shim asks whether it owns the
console rather than assuming, so a program run from a Command Prompt or a
script does not wait.

## Strip the saved program — off

`-s` at link time drops the symbol table and debugging information.

**Why the default is off:** a smaller file is worth less than a crash that can
be diagnosed.

## Optimisation level — `-O1`

`-O0`, `-O1` or `-O2`. **Ignored while the old compiler's arithmetic is on**,
which is the shipped default: that mode is built at `-O0` whatever is saved
here, and the window greys the setting out and says so. The saved level applies
again if that option is turned off.

**Why `-O1`:** with plain arithmetic the level does not change the numbers —
every value is rounded to 32 bits as it is worked out, whatever the optimiser
does, and `tests/precision.rs` asserts that `-O0`, `-O1` and `-O2` print the
same digits. So the choice is only about speed, and `-O1` is where the speed
is: on a 400 × 400 matrix product, `-O0` runs in 0.17 s, `-O1` in 0.02 s and
`-O2` in 0.01 s, all compiling in about the same time. `-O2` buys little here
and takes more liberties with the shape of the generated code.

## Preprocess — never

The app passes `-x f77` around the file, so the language is decided by the app
and never by the file's name.

**Why the default:** gcc's suffix rules are case-sensitive, and `.FOR` in
uppercase — which is what DOS-era files are called — runs the C preprocessor,
while `.for` does not and `.f77` is not recognised as Fortran at all. So the
language a file is compiled as would otherwise depend on how its name happens
to be spelled. Under the preprocessor a `#` in column 1 is taken as a directive
and disappears, and any identifier matching a macro is substituted. Checked on
the pinned GCC 16.2: an apostrophe in a comment, the other hazard this project's
older notes name, passes cleanly there, so `#` is the live one. Preprocessing is
available for code that genuinely uses `#include` or `#ifdef`.

## Extra flags — empty

Passed to the compiler verbatim, after everything else.

**Why the default:** an escape hatch for a flag the interface does not cover.
Note that because they come last, an `-O2` typed here overrides the `-O0` that
the old compiler's arithmetic needs, while the window still shows optimisation
as not in use. Treat the box as empty unless there is a reason.

## Always on, not an option

| Flag | Reason |
|---|---|
| `-fno-range-check` | Old code deliberately overflows hex and octal constants in `DATA` statements |
| `-fallow-invalid-boz` | Same era, same habit, for BOZ literals |
| `-fmax-errors=25` | A runaway error cascade can emit hundreds of megabytes |
| `-fdiagnostics-color=never` | Diagnostics are parsed, then rendered by the app |
| `-Wall -Wsurprising -Wtabs -Wextra` | Warnings are shown in a details pane, never fatal |
| `-ffrontend-optimize` at `-O0` | `-O0` otherwise stops `.AND.`/`.OR.` skipping their right-hand side, so `IF (J .NE. 0 .AND. K/J .GT. 1)` divides by zero. It changes no arithmetic |
| `-static-libgfortran -static-libgcc`, `-static` where available | The built program must run on a machine with nothing installed on it |
| Never `-Werror` | A novice's 1985 code would not build at all |
| Never `-ffast-math` | It is the one family of flags that changes what arithmetic means |
| Never `-save-temps` | It writes files beside the source |

## What the compile line looks like

With every default in place, one source file is compiled with:

```
-c -std=legacy -ffixed-form -ffixed-line-length-72 -fdec -fno-automatic
-finit-local-zero -fno-range-check -fallow-invalid-boz -mfpmath=387
-fcheck=bounds -O0 -ffrontend-optimize -fmax-errors=25
-fdiagnostics-color=never -Wall -Wsurprising -Wtabs -Wextra
-J ./mod -I<the source's own directory> -x f77 ./src/<name>.f -x none
-o ./obj/<name>.o
```

Each file is compiled separately and the objects are then linked, matching the
object-file-then-link model of the tool this replaces — and so that a failure in
one file still reports the errors in the others.
