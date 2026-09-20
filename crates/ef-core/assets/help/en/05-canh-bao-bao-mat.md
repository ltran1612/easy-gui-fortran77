# Security warnings and antivirus

## “Windows protected your PC”

The first time you run the installer, Windows may show a blue dialog saying
*Windows protected your PC*. This does **not** mean there is a virus. It means
few people have downloaded this program yet, so Windows does not recognise it.

To continue:

1. Click the small **More info** link.
2. Click **Run anyway**.

## Antivirus removing part of the compiler

A Fortran compiler is made of several small programs, among them `f951.exe` and
`collect2.exe`. Some antivirus products occasionally mistake these for something
dangerous and delete them. This is a long-known false positive.

If the application reports *The bundled compiler is missing or damaged*, this is
the likely cause. To restore it on Windows:

1. Open **Windows Security**.
2. Go to **Virus & threat protection** → **Protection history**.
3. Find the entry mentioning Easy Fortran 77 and choose **Restore**.
4. Start the application again.

If it cannot be restored, reinstall the application.

> **Note:** this application will never add an exclusion to your antivirus for
> you. That has to be your decision.

## What this application does with your files

It only ever **reads** the source files you choose. Everything the build produces
goes into the application's own working folder.

The program you compile is different: it is your program, and it can write files
wherever its code says to. Run only source code you trust, as with any other
program.
