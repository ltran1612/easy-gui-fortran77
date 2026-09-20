# Các lỗi thường gặp

## “Line truncated” — dòng bị cắt

Fortran 77 chuẩn chỉ đọc đến **cột 72**. Nếu mã nguồn của bạn viết dài hơn, hãy
mở **Tuỳ chọn nâng cao** và đổi *Độ dài dòng* thành **132 cột**.

Ngược lại, nếu tệp của bạn có đánh số thứ tự ở cột 73–80 (thói quen thời dùng
phiếu đục lỗ), hãy giữ nguyên 72 cột. Đặt *Không giới hạn* sẽ khiến các số thứ
tự đó bị hiểu nhầm là mã lệnh và sinh ra lỗi.

## “Symbol has no IMPLICIT type”

Có một biến chưa được khai báo kiểu, thường là do gõ sai tên biến.

## “undefined reference to …”

Trình liên kết không tìm thấy một chương trình con. Nguyên nhân phổ biến nhất
là **bạn quên thêm một tệp** vào danh sách.

## Kết quả khác với ngày xưa

Các trình biên dịch thời DOS lưu biến cục bộ ở vùng nhớ tĩnh và tự đặt bằng 0.
Hãy bật tuỳ chọn *Biến cục bộ tĩnh và khởi tạo bằng 0* (mặc định đã bật).

## “STRUCTURE”, “RECORD”, “UNION”

Đây là phần mở rộng của Microsoft Fortran và DEC. Hãy bật tuỳ chọn
*Phần mở rộng DEC/Microsoft* (mặc định đã bật).

## Chương trình dừng đột ngột

Nếu mã lỗi là tràn ngăn xếp, chương trình có mảng cục bộ quá lớn. Hãy bật tuỳ
chọn **Mảng lớn**.
