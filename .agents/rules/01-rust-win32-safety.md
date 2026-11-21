# Rust、Win32 与 GDI 安全规范

本文件约束涉及 Rust 代码、Win32 窗口过程及 GDI 的开发。界面行为与用户数据契约见 [UI、皮肤与数据规范](02-ui-skin-and-data.md)，验证步骤见[构建与验证手册](../skills/kstart-build-and-verify/SKILL.md)。

## 1. 语言与代码质量

- 项目沟通、代码注释、文档注释和 Git 提交信息使用中文。Rust 标识符遵循 `snake_case` 和 `PascalCase` 等语言惯例。
- 公用绘图、配置读写和数据持久化优先复用现有实现，如 `blit_canvas_with_gdi_texts`、`apply_hrgn_safely` 和 `AppState::persist_data`。
- 本地离线构建须通过，编译器警告清零。

## 2. 窗口消息重入与状态借用

`APP_STATE` 是主线程的 `RefCell<Option<AppState>>`。同步窗口消息可能重入窗口过程，回调中的借用 panic 可能直接终止进程。

- 运行期访问 `APP_STATE` 使用 `try_borrow()` / `try_borrow_mut()`。借用失败时安全退出当前分支。仅 `main()` 初始化写入状态时可直接使用 `borrow_mut()`。
- 在借用中计算并取出窗口操作所需参数，释放借用后再调用会同步发消息、改变窗体生命周期或启动消息循环的 Win32 API。包括 `CreateWindowExW`、向应用窗体同步发送消息的 `SendMessageW`、`SetWindowRgn`、`SetWindowPos`、`ShowWindow`、`DestroyWindow` 和 `RedrawWindow`。
- 文件、目录、消息等原生对话框也要在借用外打开。使用 `DeferredDialogRequest` 之类的延迟请求传递必要参数，得到结果后再尝试借用状态。
- 适用范围包括主窗体、独立菜单、搜索窗口和管理弹窗。

## 3. 无边框缩放与区域裁剪

主窗体通过 `WM_NCCALCSIZE`（`wparam != 0` 时返回 `0`）去掉原生厚边框，使用 `HRGN` 裁去四角透明像素。

1. `WM_SIZING` 阶段窗口尺寸尚未生效，尺寸、画布和区域统一在 `WM_SIZE` 更新。
2. `WM_SIZE` 跳过 `SIZE_MINIMIZED`，优先从 `GetClientRect` 取实际客户区宽高，读取失败时回退使用 `lparam`。同步更新 `user_cfg.window_width/height` 和 `canvas.resize`。
3. 仅在尺寸与 `AppState.applied_rgn_size` 不同时重建 `HRGN`，重建前先记录尺寸以防重复进入。释放借用后通过 `apply_hrgn_safely` 调用 `SetWindowRgn(hwnd, hrgn, 1)`，并使用 `RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE` 立即刷新。
4. 保留 `WM_NCPAINT` 和非最小化 `WM_NCACTIVATE` 对原生残影的抑制，以及 `WM_WINDOWPOSCHANGING` 的 `SWP_NOCOPYBITS`。在 `WM_EXITSIZEMOVE` 复核实际客户区和已应用区域后再持久化尺寸。
5. `build_window_corner_region` 在计算期间不调用会发窗口消息的 API。按行合并连续透明段，再创建并合并矩形区域。

## 4. GDI 与图标资源

- 用 `SelectObject` 选入字体或位图后，先恢复旧对象，再调用 `DeleteObject` 和 `DeleteDC`。同样检查提前返回的路径。
- `extract_file_icon_16` 使用黑白双底 `DrawIconEx` 的差值还原透明通道，保留经典 1 位 AND 掩码和黑色轮廓。系统返回的图标和 PIDL 分别使用 `DestroyIcon`、`CoTaskMemFree` 成对释放。
- 项目使用 `windows-sys 0.52`。修改 `SHBrowseForFolderW`、`SHGetPathFromIDListW` 或 `BrowseInfoW` 的手写 FFI 时，保留 `shell32` 链接、`#[repr(C)]` 布局和 Win32 ABI。
