      PROGRAM STATMAIN
      CALL COUNTER
      CALL COUNTER
      CALL COUNTER
      END

      SUBROUTINE COUNTER
      INTEGER N
C     N is never initialised and never SAVEd. A DOS-era compiler put it in
C     static storage and zeroed it, so this counts 1, 2, 3.
      N = N + 1
      WRITE (*,*) 'CALL', N
      RETURN
      END
