# Thêm và sắp xếp tệp

## Thứ tự rất quan trọng

Thứ tự các tệp trong danh sách chính là thứ tự biên dịch và liên kết, giống như
khi bạn tạo các tệp đối tượng rồi liên kết chúng lại bằng trình biên dịch cũ.
Dùng nút **Lên** và **Xuống** để sắp xếp. Thông thường nên đặt tệp chứa
`PROGRAM` lên đầu.

## Tệp `INCLUDE`

Nếu mã nguồn của bạn có câu lệnh `INCLUDE 'COMMON.INC'`, bạn **không cần** thêm
tệp `.INC` vào danh sách. Ứng dụng tự động tìm nó trong cùng thư mục với tệp mã
nguồn đã chọn.

## Khi tệp bị di chuyển

Nếu một tệp bị đổi chỗ hoặc đổi tên, dòng tương ứng sẽ hiện màu đỏ với chữ
*Không tìm thấy tệp*. Bấm **Tìm lại tệp…** để chỉ lại vị trí mới. Danh sách của
bạn không bao giờ bị tự động xoá.

## Tệp thư viện `.LIB` và `.A`

Nếu chương trình của bạn dùng thư viện đã biên dịch sẵn, hãy thêm nó vào mục
**Thư viện** chứ không phải vào danh sách mã nguồn. Xem phần *Thư viện đã biên
dịch sẵn* để biết thêm — đặc biệt nếu tệp `.LIB` của bạn được tạo từ thời DOS.
