C ----------------------------------------------------------------------
C 1. CO BAN: bien, kieu du lieu, tinh toan, in ra man hinh.
C ----------------------------------------------------------------------
C Fortran 77 la "dinh dang co dinh": lenh bat dau tu cot 7, chu C o cot
C 1 la dong ghi chu, va chi doc den cot 72.
C ----------------------------------------------------------------------
      PROGRAM COBAN
C
C     Khai bao bien. IMPLICIT NONE buoc phai khai bao moi bien.
      IMPLICIT NONE
      INTEGER   SOLUONG
      REAL      DONGIA, THANHTIEN, THUE
      CHARACTER TEN*20
      LOGICAL   DATDUOC
C
C     Gan gia tri
      TEN     = 'Xi mang PCB40'
      SOLUONG = 120
      DONGIA  = 85.5
C
C     Tinh toan. Cac phep: + - * / va ** (luy thua)
      THANHTIEN = SOLUONG * DONGIA
      THUE      = THANHTIEN * 0.08
      DATDUOC   = THANHTIEN .GT. 1000.0
C
C     In ra. WRITE(*,*) in theo kieu tu do.
      WRITE (*,*) 'Ten hang  : ', TEN
      WRITE (*,*) 'So luong  : ', SOLUONG
C
C     Dung FORMAT de can chinh cot cho dep.
      WRITE (*,100) 'Don gia   ', DONGIA
      WRITE (*,100) 'Thanh tien', THANHTIEN
      WRITE (*,100) 'Thue 8%   ', THUE
      WRITE (*,100) 'Tong cong ', THANHTIEN + THUE
      WRITE (*,*) 'Tren 1000 :', DATDUOC
C
C     Vai ham co san
      WRITE (*,110) SQRT(2.0), ABS(-7.5), MAX(3.0, 9.0), MOD(17, 5)
C
  100 FORMAT (1X, A, ' = ', F12.2)
  110 FORMAT (1X, 'SQRT(2)=', F8.4, '  ABS(-7.5)=', F6.2,
     +        '  MAX=', F6.2, '  MOD(17,5)=', I3)
      STOP
      END
