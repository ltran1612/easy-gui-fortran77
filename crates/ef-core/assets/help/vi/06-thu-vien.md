# Thư viện đã biên dịch sẵn

## Khi nào cần đến mục này

Hầu hết chương trình chỉ cần các tệp mã nguồn `.FOR`. Nhưng đôi khi một phần mã
đã được biên dịch sẵn thành **tệp thư viện** — thường có đuôi `.LIB` hoặc `.A` —
và chương trình chỉ gọi đến chứ không chứa mã đó. Nếu vậy, hãy thêm tệp thư viện
vào mục **Thư viện**.

Nếu chương trình của bạn không có tệp như vậy, hãy bỏ qua mục này.

## Thứ tự

Thư viện luôn được liên kết **sau** các tệp mã nguồn. Nếu bạn có nhiều thư viện
và chúng gọi lẫn nhau, hãy dùng nút **Lên** và **Xuống** để sắp xếp: thư viện
được gọi phải nằm **dưới** thư viện gọi nó.

## Thư viện từ trình biên dịch thời DOS

Đây là điều quan trọng nhất cần biết, và là điều làm nhiều người mất thời gian.

Các tệp `.LIB` tạo bằng trình biên dịch thời DOS — Microsoft Fortran, Lahey,
Watcom và các trình tương tự — được lưu theo một định dạng cũ tên là **OMF**.
Trình biên dịch Fortran trong ứng dụng này chỉ đọc được định dạng hiện đại. Đây
không phải là lỗi, cũng không phải là thứ có thể sửa bằng một tuỳ chọn nào đó:
hai định dạng này đơn giản là khác nhau, và phần lớn thư viện thời DOS còn được
tạo cho máy 16-bit vốn không còn tồn tại nữa.

Khi bạn thêm một tệp như vậy, ứng dụng sẽ báo ngay chứ không để bạn chờ đến lúc
biên dịch rồi mới hiện một thông báo khó hiểu.

### Trường hợp thường gặp nhất: thư viện của chính trình biên dịch cũ

Nếu các tệp `.LIB` của bạn nằm cùng thư mục với trình biên dịch cũ và có tên
kiểu như `FORTRAN.LIB`, `MATH.LIB`, `ALTMATH.LIB` hay `DECMATH.LIB`, thì đó
**không phải mã của bạn**. Đó là thư viện chạy kèm của chính trình biên dịch cũ —
phần lo việc đọc ghi tệp, tính toán số thực và khởi động chương trình.

Bạn **không cần** những tệp đó. Ứng dụng này đã có sẵn trình biên dịch Fortran
riêng, kèm theo phần tương đương hiện đại. Chỉ cần thêm các tệp `.FOR` của bạn và
bấm biên dịch; ứng dụng lo phần còn lại.

Khi bạn thêm một tệp như vậy, ứng dụng sẽ nhận ra và nói rõ điều này.

### Vậy phải làm sao

Bạn cần **các tệp mã nguồn Fortran gốc** đã được dùng để tạo ra thư viện đó —
thường cũng là các tệp `.FOR` nằm cùng thư mục, hoặc trong một thư mục tên kiểu
như `SOURCE` hay `SRC`. Hãy thêm các tệp đó vào mục **Tệp mã nguồn** như bình
thường; ứng dụng sẽ biên dịch lại chúng và bạn không cần tệp `.LIB` nữa.

Nếu không còn mã nguồn, phần chức năng nằm trong thư viện đó sẽ phải viết lại.
Không có cách nào chuyển đổi tệp `.LIB` cũ sang dùng được.

## Các thông báo khác

- *Thư viện này được tạo cho một hệ điều hành khác* — tệp dành cho Linux nhưng
  bạn đang tạo chương trình Windows, hoặc ngược lại.
- *Được tạo cho bộ xử lý khác* — thường là thư viện 32-bit trong khi chương trình
  là 64-bit. Cần bản thư viện đúng loại.
- *Tệp này không phải là thư viện đã biên dịch* — rất có thể bạn chọn nhầm một
  tệp mã nguồn. Hãy thêm nó vào mục **Tệp mã nguồn**.

## Ứng dụng không bao giờ sửa tệp của bạn

Giống như tệp mã nguồn, tệp thư viện chỉ được **đọc**. Ứng dụng sao chép nội dung
vào thư mục làm việc riêng của nó để biên dịch, và không bao giờ ghi đè lên tệp
gốc của bạn.
