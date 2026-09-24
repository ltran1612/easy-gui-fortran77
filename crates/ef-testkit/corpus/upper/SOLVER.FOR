C     Don't let the preprocessor touch this file. The apostrophe here
C     was an unterminated character constant to older cpp; today the
C     hash below is what stops it dead.
#     A hash in column 1 would be a preprocessor directive.
      PROGRAM SOLVER
      WRITE (*,*) 'cpp did not run'
      END
