# Common errors

## “Line truncated”

Standard Fortran 77 only reads to **column 72**. If your code is written wider,
open **Advanced options** and set *Line length* to **132 columns**.

Conversely, if your files carry sequence numbers in columns 73–80 (the punched-card
habit), leave it at 72. Setting *Unlimited* makes those sequence numbers be read as
code, which produces errors.

## “Symbol has no IMPLICIT type”

A variable has no declared type — usually a misspelled variable name.

## “undefined reference to …”

The linker could not find a subroutine. By far the most common cause is that
**a file is missing** from the list.

## The numbers differ from the old results

DOS-era compilers kept local variables in static storage and zeroed them. Turn on
*Static, zero-initialised local variables* (it is on by default).

## “STRUCTURE”, “RECORD”, “UNION”

These are Microsoft Fortran and DEC extensions. Turn on *DEC/Microsoft extensions*
(it is on by default).

## The program stops suddenly

If the exit code is a stack overflow, the program has a very large local array.
Turn on **Large arrays**.
