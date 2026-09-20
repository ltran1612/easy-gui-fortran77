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

## “Index … above upper bound of …”

The program used a position past the end of a list or table. For example it
read `SPAN(7)` when `SPAN` only has three places. The message names the array
and the position.

**This is the program being stopped on purpose, and it is good news.** Without
the check it would not stop: it would read whatever happened to be stored next
in memory and carry on with that number — often a plausible-looking one, like a
zero where a load should be. The result would be wrong and nothing would say so.

The usual cause is a loop that counts one step too far, or a table declared
smaller than the data now being put into it.

If you have old code that reads past the end of an array deliberately and you
need it to run as it always has, turn off **Stop if a list or table is used
past its end** in **Advanced options**.

The file name in the message is the application's own working copy, not your
file. The array name, the position and the line number are the parts that
matter.
