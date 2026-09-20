C ----------------------------------------------------------------------
C 5. VI DU CO LOI - co y, de xem ung dung bao loi the nao.
C ----------------------------------------------------------------------
C Dong WRITE ben duoi dai hon 72 cot. Fortran 77 chi doc den cot 72,
C nen dau nhay dong cuoi bi cat mat va trinh bien dich bao loi
C "Unterminated character constant".
C
C CACH SUA: mo "Tuy chon nang cao" va doi sang 132 cot, roi dich lai.
C ----------------------------------------------------------------------
      PROGRAM LOI72
      IMPLICIT NONE
      REAL X
      X = 1.5
      WRITE (*,*) 'Day la mot dong rat dai de thu nghiem viec cat bot o cot 72 cua Fortran'
      WRITE (*,*) 'X =', X
      STOP
      END
