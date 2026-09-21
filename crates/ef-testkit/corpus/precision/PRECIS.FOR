      PROGRAM PRECIS
C     ------------------------------------------------------------------
C     Every number this program prints is a fact about IEEE 754, not an
C     opinion of whichever compiler built it. They were worked out away
C     from this toolchain and are asserted exactly, so if a default flag
C     ever changes what arithmetic means, this fails instead of quietly
C     handing someone different answers than they got yesterday.
C
C     Values are read as bit patterns through EQUIVALENCE rather than
C     printed as decimals, so no rounding in the WRITE sits between the
C     arithmetic and the assertion.
C     ------------------------------------------------------------------
      REAL R, T, H, ACC
      DOUBLE PRECISION D1, D2
      INTEGER IR, IT, IH, IACC, ID1(2), ID2(2), K
      EQUIVALENCE (R, IR)
      EQUIVALENCE (T, IT)
      EQUIVALENCE (H, IH)
      EQUIVALENCE (ACC, IACC)
      EQUIVALENCE (D1, ID1)
      EQUIVALENCE (D2, ID2)

C     A. REAL is four bytes. Were it promoted to eight, this integer
C        would overlap a different part of the value and print something
C        else entirely. This is the guard on the "double precision"
C        option staying off by default.
      R = 0.1
      WRITE (*,'(A,I12)') 'A', IR

C     B. A single-precision divide, in single precision.
      R = 1.0/3.0
      WRITE (*,'(A,I12)') 'B', IR

C     C. The oldest trap in the language: a single-precision divide
C        assigned to a DOUBLE PRECISION gains no digits. It is widened
C        after the damage, so the low half is zero.
C
C        Under the default's x87 arithmetic this holds only because the
C        compiler folds 1.0/3.0 in single precision while compiling. The
C        same divide of two variables, done at run time, keeps the x87's
C        extra digits and the low half is not zero.
      D1 = 1.0/3.0
      WRITE (*,'(A,2I12)') 'C', ID1(1), ID1(2)

C     D. The same quotient actually computed in double precision.
      D2 = 1.0D0/3.0D0
      WRITE (*,'(A,2I12)') 'D', ID2(1), ID2(2)

C     E. The smallest normal single, halved, must stay non-zero -- it
C        becomes a denormal. Zero here would mean the run time is
C        flushing denormals to zero, which is what -ffast-math turns on
C        and which silently changes results near zero.
C
C        OPAQUE is in another file, so this division cannot be folded at
C        compile time and really happens on the machine.
C
C        Flushing to zero only ever touches SSE arithmetic, so under the
C        default's x87 arithmetic this cannot catch it. It still shows
C        the denormal is kept.
      IT = 8388608
      CALL OPAQUE(T)
      H = T/2.0
      WRITE (*,'(A,I12)') 'E', IH

C     F. Ten tenths do not make one, and must not be made to. A result
C        of exactly 1.0 would mean the additions were reassociated.
      ACC = 0.0
      DO 10 K = 1, 10
         ACC = ACC + 0.1
   10 CONTINUE
      WRITE (*,'(A,I12)') 'F', IACC

C     G. Integer division truncates toward zero, and MOD takes the sign
C        of the first argument: 3, -3, -1, 1.
      WRITE (*,'(A,4I6)') 'G', 7/2, (-7)/2, MOD(-7,2), MOD(7,-2)

C     H. Mixed mode. 1/2 is an integer division and is zero before the
C        multiplication is ever reached, so this is 0.0 and not 1.0.
      R = 1/2 * 2.0
      WRITE (*,'(A,I12)') 'H', IR

      END
