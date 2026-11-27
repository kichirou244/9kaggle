# Kaggle Switcher

Ứng dụng quản lý và chuyển đổi nhiều tài khoản Kaggle, chạy ngầm ở khay hệ thống (System Tray / Taskbar) trên Windows.
Được xây dựng với **Tauri v2 + React + TypeScript + Lucide Icons**.

## 🚀 Tính năng chính

- **Chạy ngầm ở Taskbar**:
  - Icon taskbar được nhúng riêng (32x32), hiển thị rõ trên Windows.
  - Nhấp chuột trái / nhấp đúp vào icon để mở giao diện quản lý.
  - Nhấp chuột phải mở menu: trạng thái, quota, đổi acc, cài đặt, làm mới quota, backup, mở thư mục `.kaggle`, thoát.
- **Không có tài khoản "mặc định"**: tên hiển thị lấy từ username thật của Kaggle (`@username`).
- **Hỗ trợ cả 2 cách xác thực của Kaggle** (theo [docs](https://github.com/Kaggle/kaggle-api/blob/main/docs/README.md#authentication)):
  - **API Tokens**: dán token `KGAT_...`, lưu vào `~/.kaggle/access_token` (`chmod 600` trên Unix).
  - **Legacy API Credentials**: dán toàn bộ `kaggle.json`, lưu vào `~/.kaggle/kaggle.json`.
  - Khi chuyển account, loại credential cũ được xoá để Kaggle không ưu tiên nhầm.
- **Tự lấy username**: khi thêm/sửa tài khoản, app gọi Kaggle API (`security.OAuthService/IntrospectToken` cho token, quota API cho legacy) để kiểm tra credential và tự gán username — không cần điền tên.
- **Quota GPU/TPU hàng tuần**: lấy qua `kernels.KernelsApiService/GetAcceleratorQuotaStatistics`, hiển thị trong giao diện và menu chuột phải. Khi khởi động tự **quét quota cho mọi account có trong config** (local DB), có nút/menu “Quét quota tất cả acc”. Mốc thời gian hiển thị theo giờ Việt Nam (UTC+7).
- **First-run trống**: cài đặt lần đầu không có account nào, không tự import credential có sẵn trên máy.
- **Backup / khôi phục dữ liệu**: sao lưu `config.json` (chứa toàn bộ account + credential) vào thư mục `backups`, có thể khôi phục lại từ giao diện.
- **Giao diện hiện đại, tối giản**: bấm thẻ tài khoản để đổi ngay (1-click switch), đồng bộ realtime giữa taskbar và giao diện.

## 📦 File chạy trực tiếp

- **[kaggle-account-switcher.exe](kaggle-account-switcher.exe)**: file `.exe` portable độc lập chạy ngay (nằm ở thư mục gốc).
- Bộ cài đặt NSIS: `src-tauri/target/release/bundle/nsis/kaggle-account-switcher_0.1.0_x64-setup.exe`.

## 🛠️ Build từ source

```bash
pnpm install
pnpm tauri build
```
