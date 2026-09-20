# Cảnh báo bảo mật và phần mềm diệt virus

## “Windows đã bảo vệ PC của bạn”

Lần đầu chạy bộ cài đặt, Windows có thể hiện một hộp thoại màu xanh với dòng chữ
*Windows protected your PC*. Điều này **không** có nghĩa là có virus. Nó chỉ có
nghĩa là phần mềm này chưa được nhiều người tải về nên Windows chưa quen.

Cách tiếp tục:

1. Bấm dòng chữ nhỏ **More info** (Thông tin thêm).
2. Bấm nút **Run anyway** (Vẫn chạy).

## Phần mềm diệt virus xoá mất trình biên dịch

Trình biên dịch Fortran gồm nhiều tệp chương trình nhỏ, trong đó có `f951.exe` và
`collect2.exe`. Một số phần mềm diệt virus đôi khi nhận nhầm các tệp này là nguy
hiểm và xoá chúng đi. Đây là lỗi nhận dạng nhầm đã được biết đến từ lâu.

Nếu ứng dụng báo *Trình biên dịch đi kèm bị thiếu hoặc hỏng*, nhiều khả năng đây
là nguyên nhân. Cách khôi phục trên Windows:

1. Mở **Windows Security** (Bảo mật Windows).
2. Vào **Virus & threat protection** → **Protection history** (Lịch sử bảo vệ).
3. Tìm mục liên quan đến Easy Fortran 77 và chọn **Restore** (Khôi phục).
4. Mở lại ứng dụng.

Nếu không khôi phục được, hãy cài đặt lại ứng dụng.

> **Lưu ý:** ứng dụng này sẽ không bao giờ tự thêm ngoại lệ vào phần mềm diệt
> virus của bạn. Việc đó phải do bạn tự quyết định.

## Ứng dụng này làm gì với tệp của bạn

Ứng dụng chỉ **đọc** các tệp mã nguồn bạn chọn. Mọi tệp do quá trình biên dịch
tạo ra đều nằm trong thư mục làm việc riêng của ứng dụng.

Chương trình do bạn biên dịch thì khác: nó là chương trình của bạn và có thể ghi
tệp ở bất cứ đâu mà mã nguồn chỉ định. Hãy chỉ chạy những mã nguồn mà bạn tin
tưởng, giống như với bất kỳ chương trình nào khác.
