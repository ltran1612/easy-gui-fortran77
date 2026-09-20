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

## “Index … above upper bound of …”

Chương trình đã dùng tới một vị trí nằm ngoài mảng. Ví dụ nó đọc `SPAN(7)`
trong khi `SPAN` chỉ có ba phần tử. Thông báo có ghi rõ tên mảng và vị trí.

**Đây là chương trình bị dừng lại có chủ đích, và đó là điều tốt.** Nếu không
có mục kiểm tra này, chương trình sẽ không dừng: nó lấy đúng vùng nhớ nằm kế
bên rồi tính tiếp với con số đó — thường là một con số trông rất hợp lý, chẳng
hạn số 0 ở chỗ đáng lẽ phải là một tải trọng. Kết quả sẽ sai mà không có gì báo
cho bạn biết.

Nguyên nhân thường gặp là vòng lặp chạy quá một bước, hoặc mảng được khai báo
nhỏ hơn lượng dữ liệu hiện đang đưa vào.

Nếu bạn có chương trình cũ cố ý đọc quá giới hạn mảng và cần nó chạy y như
trước, hãy tắt mục **Dừng lại nếu dùng quá giới hạn của mảng** trong **Tuỳ chọn
nâng cao**.

Tên tệp trong thông báo là bản sao làm việc của ứng dụng, không phải tệp của
bạn. Tên mảng, vị trí và số dòng mới là những phần đáng quan tâm.
