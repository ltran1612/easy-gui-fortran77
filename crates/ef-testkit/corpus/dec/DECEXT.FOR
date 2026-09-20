      PROGRAM DECEXT
      STRUCTURE /POINT/
        INTEGER X
        INTEGER Y
      END STRUCTURE
      RECORD /POINT/ P
      P.X = 3
      P.Y = 4
      WRITE (*,*) 'SUM', P.X + P.Y
      END
