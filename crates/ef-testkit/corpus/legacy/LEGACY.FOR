      PROGRAM LEGACY
      INTEGER I
      REAL X
      I = -4
C     Arithmetic IF: deleted in Fortran 90.
      IF (I) 10, 20, 30
   10 WRITE (*,*) 'NEGATIVE'
      GO TO 40
   20 WRITE (*,*) 'ZERO'
      GO TO 40
   30 WRITE (*,*) 'POSITIVE'
   40 CONTINUE
C     A REAL DO variable: also deleted.
      DO 50 X = 1.0, 2.0, 0.5
         CONTINUE
   50 CONTINUE
C     Assigned GO TO: also deleted.
      ASSIGN 60 TO I
      GO TO I
   60 WRITE (*,*) 'DONE'
      END
