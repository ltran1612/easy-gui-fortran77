# Adding and ordering files

## Order matters

The order of the files in the list is the compile and link order — the same as
building object files and then linking them with your old compiler. Use **Up** and
**Down** to arrange them. The file containing `PROGRAM` usually goes first.

## `INCLUDE` files

If your code contains `INCLUDE 'COMMON.INC'`, you do **not** need to add the
`.INC` file to the list. The application looks for it in the same folder as the
source files you chose.

## When a file moves

If a file is moved or renamed, its row turns red and says *File not found*. Click
**Locate file…** to point at its new place. Nothing is ever removed from your list
automatically.

## `.LIB` and `.A` library files

If your program uses a pre-compiled library, add it under **Libraries** rather
than to the source list. See *Pre-compiled libraries* for more — especially if
your `.LIB` came from a DOS-era compiler.
