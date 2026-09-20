C ----------------------------------------------------------------------
C TINH DAM BE TONG COT THEP - DAM DON GIAN, TAI PHAN BO DEU
C Vi du de thu chuong trinh Easy Fortran 77.
C ----------------------------------------------------------------------
      PROGRAM DAMBTCT
      REAL L, Q, B, H, FY, RB
      REAL M, HO, ALFA, AS, SIGMA
C
C     So lieu dau vao
      L  = 6.0
      Q  = 25.0
      B  = 0.25
      H  = 0.60
      FY = 280000.0
      RB = 14500.0
C
      WRITE (*,*) '====================================='
      WRITE (*,*) ' TINH DAM BE TONG COT THEP'
      WRITE (*,*) '====================================='
      WRITE (*,10) 'Nhip dam        L  =', L,  ' m'
      WRITE (*,10) 'Tai phan bo     q  =', Q,  ' kN/m'
      WRITE (*,10) 'Be rong         b  =', B,  ' m'
      WRITE (*,10) 'Chieu cao       h  =', H,  ' m'
      WRITE (*,*)
C
C     Momen lon nhat giua nhip: M = q*L*L/8
      M  = Q * L * L / 8.0
      HO = H - 0.05
C
C     Dien tich cot thep can thiet (cong thuc gan dung)
      ALFA = M / (RB * B * HO * HO)
      AS   = M / (0.9 * HO * FY) * 1.0E4
C
C     Ung suat uon
      SIGMA = 6.0 * M / (B * H * H)
C
      WRITE (*,10) 'Momen Mmax      M  =', M,     ' kN.m'
      WRITE (*,10) 'Chieu cao lam vc ho=', HO,    ' m'
      WRITE (*,10) 'He so alfa         =', ALFA,  ' '
      WRITE (*,10) 'Cot thep can  As   =', AS,    ' cm2'
      WRITE (*,10) 'Ung suat uon       =', SIGMA, ' kPa'
      WRITE (*,*)
C
      IF (ALFA .GT. 0.428) THEN
         WRITE (*,*) '*** CANH BAO: tiet dien qua nho, can tang h'
      ELSE
         WRITE (*,*) 'Tiet dien dat yeu cau.'
      END IF
C
   10 FORMAT (1X, A, F12.3, A)
      STOP
      END
