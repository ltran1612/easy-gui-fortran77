C ----------------------------------------------------------------------
C 3. CHUONG TRINH CON: SUBROUTINE, FUNCTION, nhieu tep, INCLUDE.
C ----------------------------------------------------------------------
C Them CA HAI tep: CHINH.FOR truoc, roi CONGCU.FOR.
C KHONG them THAMSO.INC - ung dung tu tim thay no.
C ----------------------------------------------------------------------
      PROGRAM CHINH
      IMPLICIT NONE
      INCLUDE 'THAMSO.INC'
C
C     Phai bao kieu cua ham truoc khi dung.
      REAL    DIENTICHTRON
      INTEGER GIAITHUA
      REAL    P, Q, SO(NMAX), TB, NHO, LON
      INTEGER I, N
C
C     --- Goi SUBROUTINE bang CALL
      P = 3.0
      Q = 8.0
      WRITE (*,900) 'Truoc khi hoan doi', P, Q
      CALL HOANDOI (P, Q)
      WRITE (*,900) 'Sau khi hoan doi  ', P, Q
C
C     --- SUBROUTINE tra ve nhieu ket qua
      N = 6
      DO 10 I = 1, N
         SO(I) = REAL(I) * 1.5 - 2.0
   10 CONTINUE
      CALL THONGKE (SO, N, TB, NHO, LON)
      WRITE (*,910) TB, NHO, LON
C
C     --- Goi FUNCTION nhu mot gia tri
      WRITE (*,920) 2.0, DIENTICHTRON(2.0)
      WRITE (*,930) 5, GIAITHUA(5)
C
C     --- Hang so lay tu THAMSO.INC
      WRITE (*,940) PI, TRONGLUONGRIENG
C
  900 FORMAT (1X, A, ':  P =', F6.2, '   Q =', F6.2)
  910 FORMAT (1X, 'Trung binh =', F7.3, '  Nho nhat =', F7.3,
     +        '  Lon nhat =', F7.3)
  920 FORMAT (1X, 'Dien tich tron duong kinh', F5.1, ' =', F8.4)
  930 FORMAT (1X, 'Giai thua cua', I3, ' =', I8)
  940 FORMAT (1X, 'PI =', F10.7, '   Trong luong rieng thep =', F6.2)
      STOP
      END
