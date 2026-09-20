# Saving and running your program

This application **compiles** your program for you, but it does not run it. Once
the build succeeds, you decide where to keep the program and when to run it.

## Saving the program

After a successful build, click **Save the program…**. A folder chooser appears.
Pick where you want to keep it — your **Documents** folder, or the **Desktop**.

If you do not save it, the file is deleted when you close the application,
because it is built in a temporary working folder.

## How to run it

A Fortran program is a **command-line** program. If you double-click it in
Windows Explorer, a black window appears, the program runs, and the window
**closes immediately** — too fast to read the results.

There are two ways to see the output:

### Option 1: open a Command Prompt in the folder

1. Open the folder where you saved the program.
2. Click the address bar at the top, type `cmd`, and press **Enter**.
3. A black window appears. Type the program's name and press **Enter**.

That window stays open after the program finishes, so you can read the results
and type in data when the program asks for it.

### Option 2: make a `.bat` file to run it

Create a text file in the same folder, named for example `run.bat`, containing:

```
@echo off
YourProgram.exe
pause
```

Replace `YourProgram.exe` with your program's name. From then on, just
double-click `run.bat`. The `pause` line keeps the window open until you press a
key.

## Output files

If the program writes results with `OPEN` and `WRITE` using a plain file name,
that file lands in **the folder you ran the program from**.
