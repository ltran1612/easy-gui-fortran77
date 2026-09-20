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

### Cách dễ nhất: dùng tệp `.BAT` mà ứng dụng đã tạo sẵn

Khi lưu chương trình, ứng dụng lưu kèm một tệp `.BAT` cùng tên ngay bên cạnh.
**Hãy bấm đúp vào tệp `.BAT` đó** thay vì vào tệp `.EXE`. Cửa sổ sẽ ở lại sau khi
chương trình chạy xong, và bạn nhập được số liệu khi chương trình yêu cầu.

Nếu bạn không muốn tạo tệp này, hãy tắt tuỳ chọn trong **Cài đặt**.

### Hoặc: để chính chương trình tự chờ

Trong **Tuỳ chọn nâng cao** có mục **Chờ nhấn phím trước khi đóng cửa sổ**. Bật
nó lên thì chính tệp `.EXE` sẽ chờ bạn nhấn Enter, nên bạn chỉ cần một tệp duy
nhất, không cần tệp `.BAT` nữa.

Chương trình chỉ chờ khi bạn bấm đúp vào nó. Nếu chạy từ Command Prompt hoặc từ
một tệp `.BAT` khác thì nó chạy xong là xong, không chờ gì cả.

### Cách khác: mở Command Prompt trong thư mục chứa chương trình

1. Mở thư mục bạn vừa lưu chương trình.
2. Bấm vào thanh địa chỉ ở phía trên, gõ `cmd` rồi nhấn **Enter**.
3. Một cửa sổ đen hiện ra. Gõ tên chương trình rồi nhấn **Enter**.

Cửa sổ này sẽ ở lại sau khi chương trình kết thúc, nên bạn đọc được kết quả và
nhập được số liệu khi chương trình yêu cầu.

## Tệp kết quả

Nếu chương trình ghi ra tệp kết quả bằng `OPEN` và `WRITE` với tên tệp không kèm
đường dẫn, tệp đó sẽ nằm trong **cùng thư mục nơi bạn chạy chương trình**.
