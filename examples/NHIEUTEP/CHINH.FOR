C     Chuong trinh chinh - tinh mot loat dam.
      PROGRAM CHINH
      INCLUDE 'THAMSO.INC'
      REAL L(NMAX), Q, B, H, M, W, ATHEP
      REAL DTHINHTRON
      INTEGER I, N
C
      N = 5
      B = 0.22
      H = 0.50
      Q = 18.0
      DO 20 I = 1, N
         L(I) = 3.0 + 0.5 * FLOAT(I)
   20 CONTINUE
C
      CALL TRONGLUONG (B, H, W)
      WRITE (*,*) 'Trong luong ban than =', W, ' kN/m'
      WRITE (*,*) 'Dien tich thep D20   =', DTHINHTRON(2.0), ' cm2'
      WRITE (*,*)
      WRITE (*,*) '   Nhip(m)     Momen(kN.m)'
C
      DO 30 I = 1, N
         CALL MOMEN (Q + W, L(I), M)
         WRITE (*,40) L(I), M
   30 CONTINUE
C
   40 FORMAT (1X, F10.2, F15.3)
      STOP
      END
