# KStart 项目指南

KStart 是基于 Rust、Win32 和 GDI 的 Windows 桌面启动器。项目包含皮肤引擎、分类和栏目、快捷方式管理、搜索与热键。

## 从这里开始

按任务查阅下表的规则和手册，再修改相关模块。[`.agents/rules/`](.agents/rules/) 定义项目约束，[`.agents/skills/`](.agents/skills/) 提供操作步骤和验收方法。

| 任务 | 必读规则 | 操作与验收 | 代码入口 |
| --- | --- | --- | --- |
| Win32 窗体、消息循环、缩放、裁剪或 GDI | [Win32 / GDI 安全](.agents/rules/01-rust-win32-safety.md) | [构建与验证](.agents/skills/kstart-build-and-verify/SKILL.md) | `src/main.rs`、`src/renderer.rs`、`src/skin_assets.rs` |
| 创建或修改弹出菜单、对话框 | [弹出菜单与对话框](.agents/rules/02-ui-skin-and-data.md#3-弹出菜单与对话框)、[Win32 / GDI 安全](.agents/rules/01-rust-win32-safety.md) | [构建与验证](.agents/skills/kstart-build-and-verify/SKILL.md) | `src/popup.rs`、`src/main.rs`、`src/renderer.rs`、`src/native_dialogs.rs` |
| 搜索、启动快捷方式及键盘操作 | [控件与搜索行为](.agents/rules/02-ui-skin-and-data.md#4-控件与搜索行为)、[Win32 / GDI 安全](.agents/rules/01-rust-win32-safety.md) | [构建与验证](.agents/skills/kstart-build-and-verify/SKILL.md) | `src/search.rs`、`src/search_dialog.rs`、`src/main.rs` |
| 高级设置、TrayStart 和系统功能 | [UI、皮肤与用户数据规范](.agents/rules/02-ui-skin-and-data.md)、[Win32 / GDI 安全](.agents/rules/01-rust-win32-safety.md) | [构建与验证](.agents/skills/kstart-build-and-verify/SKILL.md) | `src/advanced.rs`、`src/advanced_dialog.rs`、`src/advanced_runtime.rs` |
| 新增、迁移或排查皮肤和 `Setup.ini` | [皮肤发现与加载](.agents/rules/02-ui-skin-and-data.md#1-皮肤发现与加载)、[配置契约](.agents/rules/02-ui-skin-and-data.md#2-setupini-配置契约) | [皮肤编写](.agents/skills/kstart-skin-authoring/SKILL.md) | `skin/`、`src/skin_parser.rs`、`src/skin_assets.rs` |
| 类别、栏目、快捷方式、拖放、热键和持久化 | [UI、皮肤与数据规范](.agents/rules/02-ui-skin-and-data.md)。涉及窗口消息或系统对话框时查阅 [Win32 / GDI 安全](.agents/rules/01-rust-win32-safety.md) | [构建与验证](.agents/skills/kstart-build-and-verify/SKILL.md) | `src/model.rs`、`src/config.rs`、`src/main.rs` |
| 离线构建、截图及实机回归 | [Win32 / GDI 安全](.agents/rules/01-rust-win32-safety.md)、[用户数据契约](.agents/rules/02-ui-skin-and-data.md#5-用户数据契约) | [构建与验证](.agents/skills/kstart-build-and-verify/SKILL.md) | `Cargo.toml`、`src/main.rs` |

## 项目地图

| 路径 | 职责 |
| --- | --- |
| [`src/main.rs`](src/main.rs) | 应用入口、窗口消息分发和运行状态 |
| [`src/popup.rs`](src/popup.rs)、[`src/native_dialogs.rs`](src/native_dialogs.rs) | 弹出菜单窗口和管理对话框控件 |
| [`src/renderer.rs`](src/renderer.rs) | 画布绘制、文字指令与命中测试 |
| [`src/search.rs`](src/search.rs)、[`src/search_dialog.rs`](src/search_dialog.rs)、[`src/main_search.rs`](src/main_search.rs) | 搜索模式与匹配、结果窗口、主界面原生搜索输入框 |
| [`src/schedule_reminder.rs`](src/schedule_reminder.rs) | 计划提醒窗口、60 秒自动关闭与声音播放 |
| [`src/config.rs`](src/config.rs)、[`src/model.rs`](src/model.rs) | 配置与热键、分类和快捷方式数据 |
| [`src/schedule.rs`](src/schedule.rs)、[`src/schedule_runtime.rs`](src/schedule_runtime.rs)、[`src/schedule_dialog.rs`](src/schedule_dialog.rs) | `user/plan.json`、计划触发与批处理执行、计划任务管理窗口 |
| [`src/skin_parser.rs`](src/skin_parser.rs)、[`src/skin_assets.rs`](src/skin_assets.rs) | 皮肤字段与锚点、位图与图标加载 |
| [`skin/`](skin/)、[`assets/`](assets/) | 皮肤包及内置公共资源 |
| [`user/`](user/) | 用户运行期配置和数据 |
