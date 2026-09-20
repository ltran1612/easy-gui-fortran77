      SUBROUTINE OPAQUE(X)
C     Does nothing, on purpose.
C
C     It lives in its own file because the application compiles each file
C     separately and links the objects, exactly as the user's old tool
C     did. The compiler therefore cannot see that X comes back unchanged,
C     so it cannot fold away the arithmetic that follows the call. That
C     is what makes a run-time rounding mode observable rather than
C     evaluated at compile time, where it would not apply.
      REAL X
      RETURN
      END
