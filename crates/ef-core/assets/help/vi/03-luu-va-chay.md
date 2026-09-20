# Lưu và chạy chương trình

Ứng dụng này **biên dịch** chương trình cho bạn, nhưng không chạy nó. Sau khi
biên dịch xong, bạn tự quyết định lưu chương trình ở đâu và chạy khi nào.

## Lưu chương trình

Sau khi biên dịch thành công, bấm **Lưu chương trình…**. Một cửa sổ chọn thư mục
sẽ hiện ra. Hãy chọn nơi bạn muốn lưu, ví dụ thư mục **Tài liệu** hoặc ngay trên
**Màn hình nền (Desktop)**.

Nếu không lưu, tệp chương trình sẽ bị xoá khi bạn đóng ứng dụng, vì nó được tạo
ra trong thư mục làm việc tạm thời.

## Cách chạy chương trình

Chương trình Fortran là chương trình **dòng lệnh**. Nếu bạn bấm đúp vào nó trong
Windows Explorer, một cửa sổ đen sẽ hiện ra, chạy xong rồi **đóng lại ngay lập
tức** — bạn sẽ không kịp đọc kết quả.

Có hai cách để xem được kết quả:

### Cách 1: Mở Command Prompt trong thư mục chứa chương trình

1. Mở thư mục bạn vừa lưu chương trình.
2. Bấm vào thanh địa chỉ ở phía trên, gõ `cmd` rồi nhấn **Enter**.
3. Một cửa sổ đen hiện ra. Gõ tên chương trình rồi nhấn **Enter**.

Cửa sổ này sẽ ở lại sau khi chương trình kết thúc, nên bạn đọc được kết quả và
nhập được số liệu khi chương trình yêu cầu.

### Cách 2: Tạo một tệp `.bat` để chạy kèm

Tạo một tệp văn bản trong cùng thư mục, đặt tên ví dụ `chay.bat`, nội dung:

```
@echo off
TenChuongTrinh.exe
pause
```

Thay `TenChuongTrinh.exe` bằng tên chương trình của bạn. Từ nay chỉ cần bấm đúp
vào `chay.bat`. Dòng `pause` giữ cửa sổ mở lại cho đến khi bạn nhấn một phím.

## Tệp kết quả

Nếu chương trình ghi ra tệp kết quả bằng `OPEN` và `WRITE` với tên tệp không kèm
đường dẫn, tệp đó sẽ nằm trong **cùng thư mục nơi bạn chạy chương trình**.
