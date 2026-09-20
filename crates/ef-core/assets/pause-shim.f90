! Keep the console window open when the program is double-clicked.
!
! A Fortran program launched from Explorer gets a console of its own, and
! Windows destroys that console the moment the program exits -- too fast to read
! anything. This waits first.
!
! Linked in only when the option is on, and never compiled into the user's own
! source: it is a separate object, and `-Wl,--wrap=exit` is what routes exit()
! through it. Nothing in their program changes, and without the link flag this
! file is inert.
!
! Free form and modern Fortran on purpose. It is ours, not theirs, so it is
! compiled with its own flags rather than the legacy ones their code needs.
module ef77_pause
  use iso_c_binding
  implicit none
contains

  ! True when this program owns its console -- i.e. Windows created one for it
  ! because someone double-clicked. Started from cmd.exe, a .bat or another
  ! program, the console already had a process in it and this is false, so a
  ! script never hangs waiting for a key nobody is there to press.
  logical function owns_its_console()
    integer(c_int32_t) :: procs(4), n
    interface
      function gcpl(list, count) bind(C, name="GetConsoleProcessList") result(r)
        import :: c_int32_t
        integer(c_int32_t) :: list(*)
        integer(c_int32_t), value :: count
        integer(c_int32_t) :: r
      end function
    end interface
    n = gcpl(procs, 4_c_int32_t)
    owns_its_console = (n == 1)
  end function

  subroutine wrap_exit(code) bind(C, name="__wrap_exit")
    integer(c_int), value :: code
    integer :: ios
    character(len=1) :: key
    interface
      subroutine real_exit(k) bind(C, name="__real_exit")
        import :: c_int
        integer(c_int), value :: k
      end subroutine
    end interface
    if (owns_its_console()) then
      write (*, '(a)') ''
      write (*, '(a)', advance='no') &
        'Chuong trinh da ket thuc. Nhan Enter de dong. / Press Enter to close: '
      read (*, '(a)', iostat=ios) key
    end if
    call real_exit(code)
  end subroutine

end module
