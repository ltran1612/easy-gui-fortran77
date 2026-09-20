# Saving and running your program

This application **compiles** your program for you, but it does not run it. Once
the build succeeds, you decide where to keep the program and when to run it.

## Saving the program

After a successful build, click **Save the program…**. A folder chooser appears.
Pick where you want to keep it — your **Documents** folder, or the **Desktop**.

If you do not save it, the file is deleted when you close the application,
because it is built in a temporary working folder.

## How to run it

**Double-click the `.EXE`.** A black window appears, the program runs, and then
it **waits for you to press Enter** before the window closes, so you have as
long as you like to read the results.

That waiting is an option — **Wait for a key before the window closes**, under
**Advanced options** — and it is on unless you turn it off. It applies only when
you double-click the program. Started from a Command Prompt or from a script, the
program finishes and exits as usual, because the window it is running in was not
going anywhere.

The waiting is built into the program itself, so it travels with it. You can
email the `.EXE` to someone or copy it to a memory stick, and it still waits.

### The other way: open a Command Prompt in the folder

1. Open the folder where you saved the program.
2. Click the address bar at the top, type `cmd`, and press **Enter**.
3. A black window appears. Type the program's name and press **Enter**.

That window stays open after the program finishes, so you can read the results
and type in data when the program asks for it.

## Output files

If the program writes results with `OPEN` and `WRITE` using a plain file name,
that file lands in **the folder you ran the program from**.
