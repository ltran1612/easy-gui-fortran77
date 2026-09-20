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

**Bấm đúp vào tệp `.EXE`.** Một cửa sổ đen hiện ra, chương trình chạy, rồi
**chờ bạn nhấn Enter** mới đóng lại, nên bạn có bao nhiêu thời gian tuỳ ý để đọc
kết quả.

Việc chờ này là một tuỳ chọn — **Chờ nhấn phím trước khi đóng cửa sổ** trong
**Tuỳ chọn nâng cao** — và nó được bật sẵn, trừ khi bạn tự tắt đi. Nó chỉ áp
dụng khi bạn bấm đúp vào chương trình. Nếu chạy từ Command Prompt hoặc từ một
tệp `.BAT` thì chương trình chạy xong là xong, không chờ gì cả, vì cửa sổ đó vốn
không tự đóng.

### Tệp `.BAT` mà ứng dụng đã tạo sẵn

Khi lưu chương trình, ứng dụng lưu kèm một tệp `.BAT` cùng tên ngay bên cạnh.
Bấm đúp vào tệp đó cũng cho kết quả tương tự, và vẫn dùng được kể cả khi sau này
bạn tắt tuỳ chọn chờ nhấn phím.

Nếu bạn không muốn tạo tệp này, hãy tắt tuỳ chọn trong **Cài đặt**.

### Cách khác: mở Command Prompt trong thư mục chứa chương trình

1. Mở thư mục bạn vừa lưu chương trình.
2. Bấm vào thanh địa chỉ ở phía trên, gõ `cmd` rồi nhấn **Enter**.
3. Một cửa sổ đen hiện ra. Gõ tên chương trình rồi nhấn **Enter**.

Cửa sổ này sẽ ở lại sau khi chương trình kết thúc, nên bạn đọc được kết quả và
nhập được số liệu khi chương trình yêu cầu.

## Tệp kết quả

Nếu chương trình ghi ra tệp kết quả bằng `OPEN` và `WRITE` với tên tệp không kèm
đường dẫn, tệp đó sẽ nằm trong **cùng thư mục nơi bạn chạy chương trình**.
