C ----------------------------------------------------------------------
C 2. VONG LAP VA RE NHANH: DO, IF/ELSE, mang.
C ----------------------------------------------------------------------
      PROGRAM VONGLAP
      IMPLICIT NONE
      INTEGER I, J, N, TONG
      REAL    DIEM(10), TB, CAONHAT
      CHARACTER XEPLOAI*10
C
      N = 8
C
C     --- Vong lap DO: tinh tong 1 + 2 + ... + N
      TONG = 0
      DO 10 I = 1, N
         TONG = TONG + I
   10 CONTINUE
      WRITE (*,*) 'Tong 1..', N, ' =', TONG
C
C     --- DO co buoc nhay: dem nguoc tu 10 ve 0, moi lan 2
      WRITE (*,*) 'Dem nguoc:'
      DO 20 I = 10, 0, -2
         WRITE (*,900) I
   20 CONTINUE
C
C     --- Mang: gan roi tim trung binh va lon nhat
      DIEM(1) = 7.5
      DIEM(2) = 6.0
      DIEM(3) = 9.25
      DIEM(4) = 4.5
      DIEM(5) = 8.0
      N = 5
C
      TB      = 0.0
      CAONHAT = DIEM(1)
      DO 30 I = 1, N
         TB = TB + DIEM(I)
         IF (DIEM(I) .GT. CAONHAT) CAONHAT = DIEM(I)
   30 CONTINUE
      TB = TB / REAL(N)
      WRITE (*,910) TB, CAONHAT
C
C     --- IF / ELSE IF / ELSE
      DO 40 I = 1, N
         IF (DIEM(I) .GE. 8.0) THEN
            XEPLOAI = 'Gioi'
         ELSE IF (DIEM(I) .GE. 6.5) THEN
            XEPLOAI = 'Kha'
         ELSE IF (DIEM(I) .GE. 5.0) THEN
            XEPLOAI = 'Trung binh'
         ELSE
            XEPLOAI = 'Yeu'
         END IF
         WRITE (*,920) I, DIEM(I), XEPLOAI
   40 CONTINUE
C
C     --- Vong lap long nhau: bang cuu chuong 2 va 3
      WRITE (*,*) 'Bang nhan:'
      DO 60 I = 2, 3
         DO 50 J = 1, 5
            WRITE (*,930) I, J, I*J
   50    CONTINUE
   60 CONTINUE
C
  900 FORMAT (1X, '  ', I3)
  910 FORMAT (1X, 'Trung binh =', F6.2, '   Cao nhat =', F6.2)
  920 FORMAT (1X, 'Diem ', I2, ' =', F6.2, '  ->  ', A)
  930 FORMAT (1X, I3, ' x', I3, ' =', I4)
      STOP
      END
