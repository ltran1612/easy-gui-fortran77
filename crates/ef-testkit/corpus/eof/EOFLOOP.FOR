      PROGRAM EOFLOOP
      INTEGER V, TOT
      TOT = 0
   10 READ (*,*,END=20) V
      TOT = TOT + V
      GO TO 10
   20 WRITE (*,*) 'TOTAL', TOT
      END
