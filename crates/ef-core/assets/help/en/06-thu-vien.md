# Pre-compiled libraries

## When you need this

Most programs need only their `.FOR` source files. Sometimes, though, part of the
code has already been compiled into a **library file** — usually ending in `.LIB`
or `.A` — and the program only calls into it. When that is the case, add the
library file under **Libraries**.

If your program has no such file, you can ignore this section.

## Order

Libraries are always linked **after** the source files. If you have several that
call one another, use **Up** and **Down** to order them: a library must appear
*below* whatever calls into it.

## Libraries from a DOS-era compiler

This is the most important thing to know, and the thing that costs people the
most time.

A `.LIB` file produced by a DOS-era compiler — Microsoft Fortran, Lahey, Watcom
and their contemporaries — is stored in an old format called **OMF**. The Fortran
compiler in this application reads only the modern format. This is not a fault
and not something a setting can change: the two formats are simply different, and
most DOS-era libraries were additionally built for 16-bit machines that no longer
exist.

When you add such a file the application says so immediately, rather than letting
you wait for a build and then showing an error that explains nothing.

### The most common case: the old compiler's own library

If your `.LIB` files sit in the same folder as the old compiler and have names
like `FORTRAN.LIB`, `MATH.LIB`, `ALTMATH.LIB` or `DECMATH.LIB`, they are **not
your code**. They are the old compiler's own runtime library — the part that
handled reading and writing files, floating-point arithmetic and program startup.

You do **not** need them. This application brings its own Fortran compiler, which
includes the modern equivalent. Just add your `.FOR` files and press compile.

The application recognises these files and says so when you add one.

### What to do instead

You need the **original Fortran source files** the library was built from —
usually more `.FOR` files, in the same folder or in one called something like
`SOURCE` or `SRC`. Add those under **Source files** as normal; the application
compiles them again and the `.LIB` is no longer needed.

If the source is genuinely gone, the code in that library has to be rewritten.
There is no way to convert an old `.LIB` into something usable.

## The other messages

- *Built for a different operating system* — the file is for Linux while you are
  building a Windows program, or the other way round.
- *Built for a different processor* — usually a 32-bit library against a 64-bit
  program. You need the matching build of the library.
- *This file is not a compiled library* — most likely a source file chosen by
  mistake. Add it under **Source files**.

## Your files are never modified

Like source files, library files are only ever **read**. The application copies
the contents into its own working folder to compile, and never writes over your
original.
