---
name: kstart-build-and-verify
description: 编译 KStart、导出离线界面截图，或验证 Win32 缩放、裁剪、菜单、搜索和对话框交互时使用。
---

# KStart 构建与验证

核对当前任务适用的 [Win32 / GDI 安全规范](../../rules/01-rust-win32-safety.md) 和 [UI、皮肤与数据规范](../../rules/02-ui-skin-and-data.md)。按变更选择离线截图、实机交互或两者。

## 1. 离线构建

在项目根目录运行 `cargo build --offline`，确认退出码和编译器输出。涉及已有测试覆盖的逻辑时运行 `cargo test --offline`。

若默认目录的可执行文件正在使用，可指定独立的 `--target-dir`。实机测试应启动本次构建产生的可执行文件。

## 2. 离线截图

`--dump-screenshot` 输出 PNG 后退出。截图、日志和临时脚本写入会话 scratch 或系统临时目录。

```powershell
$kstartShot = Join-Path $env:TEMP 'kstart-main.png'
cargo run --offline -- --dump-screenshot $kstartShot
```

可在应用参数中加以下开关。核对实际生成的文件、尺寸和画面。

| 场景 | 额外参数 |
| --- | --- |
| 第 2 类别 | `--cat2` |
| 主菜单、列表右键菜单、快捷方式右键菜单、皮肤菜单预览 | `--open-menu`、`--open-ctx-menu`、`--open-item-menu`、`--open-skin-menu` |
| 框选或多选高亮 | `--demo-marquee`、`--demo-selected` |
| 关于、编辑快捷方式、备注、类别管理的绘制预览 | `--modal-about`、`--modal-edit`、`--modal-edit-remark`、`--modal-cat` |

离线菜单截图会合成在主窗体上。需要确认独立窗口、屏幕边缘定位或原生控件时，使用实机验证。编辑预览需要测试数据中存在相应的快捷方式。

## 3. 实机验证

1. 在会话 scratch 准备测试工作目录，放入所需的 `skin/`、`assets/` 和隔离的 `user/` 数据。以该目录作为 `Start-Process -WorkingDirectory`。使用真实数据样本时复制到测试目录。
2. 使用 `Start-Process -WindowStyle Hidden -PassThru` 启动并重定向输出与错误日志。按测试进程 PID 枚举窗口，等待目标窗口出现并确认进程仍在运行。主窗体为 `KStartKStart5WindowClass`，菜单为 `KStartPopupMenuClass`，对话框类名见 [UI 规则](../../rules/02-ui-skin-and-data.md#3-弹出菜单与对话框)。
3. 并列核对任务相关结果：
   - **窗体缩放与裁剪**：使用 `SetWindowPos` 测试缩小和放大，比较 `GetWindowRect`、`GetClientRect` 和 `GetWindowRgnBox` 的宽高。用 `PrintWindow` 检查边框和透明角。
   - **菜单**：通过对应按钮或右键操作打开菜单，以菜单自身的句柄检查窗口位置、皮肤绘制和点击命中。检查靠近屏幕边缘时的定位、适用的滚动及关闭行为。
   - **搜索**：检查输入时主列表保持原样、回车出现结果、首项选中、方向键切换和回车执行。用隔离数据中的测试快捷方式验证执行。
   - **数据提交**：检查确认、取消和运行期持久化，仅比较测试目录中的前后数据。
4. 用 `try/finally` 清理本次启动的测试进程，通过保存的 PID 或进程对象确认退出。保留用户原有的 KStart 进程。
