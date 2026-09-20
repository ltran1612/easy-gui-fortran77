C ----------------------------------------------------------------------
C Cac chuong trinh con. Tep nay khong co PROGRAM.
C ----------------------------------------------------------------------
C SUBROUTINE: goi bang CALL, tra ket qua qua tham so.
C FUNCTION  : goi nhu mot gia tri, tra dung mot ket qua.
C ----------------------------------------------------------------------
C
      SUBROUTINE HOANDOI (A, B)
C     Doi cho hai so. Tham so trong Fortran la "truyen theo dia chi",
C     nen sua A va B o day la sua luon ben ngoai.
      IMPLICIT NONE
      REAL A, B, TAM
      TAM = A
      A   = B
      B   = TAM
      RETURN
      END
C
      SUBROUTINE THONGKE (X, N, TB, NHONHAT, LONNHAT)
C     Tra ve nhieu ket qua cung luc - dieu FUNCTION khong lam duoc.
      IMPLICIT NONE
      INTEGER N, I
      REAL X(N), TB, NHONHAT, LONNHAT
      TB      = 0.0
      NHONHAT = X(1)
      LONNHAT = X(1)
      DO 10 I = 1, N
         TB = TB + X(I)
         IF (X(I) .LT. NHONHAT) NHONHAT = X(I)
         IF (X(I) .GT. LONNHAT) LONNHAT = X(I)
   10 CONTINUE
      TB = TB / REAL(N)
      RETURN
      END
C
      REAL FUNCTION DIENTICHTRON (D)
C     Dien tich hinh tron theo duong kinh.
      IMPLICIT NONE
      INCLUDE 'THAMSO.INC'
      REAL D
      DIENTICHTRON = PI * D * D / 4.0
      RETURN
      END
C
      INTEGER FUNCTION GIAITHUA (N)
C     Vi du ve vong lap trong ham.
      IMPLICIT NONE
      INTEGER N, I, KQ
      KQ = 1
      DO 20 I = 2, N
         KQ = KQ * I
   20 CONTINUE
      GIAITHUA = KQ
      RETURN
      END
