# Getting started

This application compiles and runs your Fortran 77 programs without needing a
command-line window.

## Three steps

1. Click **New program** and give it a name, for example *Beam calculation*.
2. Click **Add files…** and choose your `.FOR` or `.F` files.
3. Click the green **Compile program** button.
4. Click **Save the program…** and choose where to keep it.

If anything is wrong, the messages appear in the panel below with a plain-language
explanation. See *Saving and running* for how to run the program you saved.

## The application never changes your files

Your source files are only ever **read**. The application copies them into its own
working folder and compiles the copies. Your originals are untouched, even when
the build fails.

That is also why this application has **no source editor**. To change your code,
open the file in Notepad or whichever editor you already use.
