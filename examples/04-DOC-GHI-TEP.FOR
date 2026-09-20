C ----------------------------------------------------------------------
C 4. DOC VA GHI TEP: OPEN, WRITE, READ, CLOSE.
C ----------------------------------------------------------------------
C Chuong trinh nay tao ra tep SOLIEU.TXT ben canh no, roi doc lai.
C Day la cach cac chuong trinh cu nhan so lieu vao va xuat ket qua ra.
C ----------------------------------------------------------------------
      PROGRAM DOCGHI
      IMPLICIT NONE
      INTEGER I, N, IOS, SOHIEU
      REAL    X, TONG
      CHARACTER NHAN*5
C
C     --- GHI: mo tep de ghi, so hieu 10 la tuy chon
      OPEN (UNIT=10, FILE='SOLIEU.TXT', STATUS='UNKNOWN')
      WRITE (10,*) 5
      DO 10 I = 1, 5
         WRITE (10,800) 'Diem-', I, REAL(I) * 2.5
   10 CONTINUE
      CLOSE (10)
      WRITE (*,*) 'Da ghi xong tep SOLIEU.TXT'
C
C     --- DOC: mo lai tep vua ghi
      OPEN (UNIT=11, FILE='SOLIEU.TXT', STATUS='OLD', IOSTAT=IOS)
      IF (IOS .NE. 0) THEN
         WRITE (*,*) 'Khong mo duoc tep SOLIEU.TXT'
         STOP
      END IF
C
      READ (11,*) N
      WRITE (*,*) 'Trong tep co', N, ' dong so lieu'
      TONG = 0.0
      DO 20 I = 1, N
C        Doc lai dung theo dinh dang da ghi: du CA BA muc, dung thu tu.
         READ (11,800) NHAN, SOHIEU, X
         WRITE (*,810) NHAN, SOHIEU, X
         TONG = TONG + X
   20 CONTINUE
      CLOSE (11)
C
      WRITE (*,820) TONG, TONG / REAL(N)
C
C     IOSTAT khac 0 nghia la het tep hoac co loi - cach doc den het tep
C     ma khong biet truoc so dong.
C
  800 FORMAT (A5, I2, 1X, F10.3)
  810 FORMAT (1X, 'Doc duoc: ', A5, I2, ' =', F10.3)
  820 FORMAT (1X, 'Tong =', F10.3, '   Trung binh =', F10.3)
      STOP
      END
