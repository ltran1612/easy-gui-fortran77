C     Cac chuong trinh con dung chung.
C
      SUBROUTINE MOMEN (Q, L, M)
      REAL Q, L, M
      M = Q * L * L / 8.0
      RETURN
      END
C
      SUBROUTINE TRONGLUONG (B, H, W)
      INCLUDE 'THAMSO.INC'
      REAL B, H, W
      W = B * H * GAMMA
      RETURN
      END
C
      REAL FUNCTION DTHINHTRON (D)
      INCLUDE 'THAMSO.INC'
      REAL D
      DTHINHTRON = PI * D * D / 4.0
      RETURN
      END
