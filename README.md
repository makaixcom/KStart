# KStart

基于 Rust、Win32 和 GDI 的 Windows 桌面启动器，复刻经典「音速启动 / VStart」的使用方式与皮肤界面。

音速启动是我儿时很喜欢的软件。因为原版长期没有更新，在 Windows 11 上的使用体验也受到影响，所以有了这个项目。

## 功能

- 分类与栏目管理，文件、文件夹、网址和系统功能快捷方式。
- 拖入文件和 Windows 快捷方式，框选、多选、拖动排序及跨栏目移动。
- 按标题、路径或加入当前栏目的时间排序，支持升序和降序。
- 批量删除、修改目录、替换图标和设置运行后最小化。
- 搜索和导入 `.lnk`、`.url`，导出 Windows 快捷方式和完整 KStart 配置包。
- 搜索、全局热键、托盘、靠边隐藏及高级设置。
- 本地皮肤切换、等比预览、作者说明和皮肤删除。网络皮肤页暂待开发。

## 下载与使用

在 [Releases](https://github.com/makaixcom/KStart/releases) 下载 `KStart-版本号-windows-x64.zip`，完整解压后运行 `kstart.exe`。程序需要同目录的皮肤和资源文件，不能只复制一个 EXE。

```text
KStart/
├─ kstart.exe
├─ skin/          皮肤包
├─ assets/        内置公共资源
├─ icons/         公共图标
└─ user/          首次运行时自动生成的配置、快捷方式和缓存
```

将目录放在当前用户有写入权限的位置。升级前备份原目录的 `user/`，解压新版本时保留该目录。

右键快捷方式列表的空白处，可以打开快捷方式排序、批量操作/导出/导入及修改栏目标题窗口。

“添加时间”表示条目加入**当前栏目**的时间；跨栏目移动、粘贴和导入会重新记录。同栏内排序或修改资料不会重记。旧版数据没有历史入栏时间，升级后以原有显示顺序作为基准。

导出时会保存完整的 `*.kstart.json` 配置包，并为文件、文件夹和网址生成 `.lnk` / `.url`。恢复图标、备注、文字颜色等完整设置时，在导入页将搜索类型切换为 `*.kstart.json`。导入目标始终是打开管理窗口时的栏目。

## 从源码构建

在 Windows 上安装 [Rust](https://www.rust-lang.org/tools/install) 和 Visual Studio Build Tools 的“使用 C++ 的桌面开发”工作负载（包含 MSVC 和 Windows SDK）。项目使用 Windows API，需要在 Windows 上编译。

```powershell
git clone https://github.com/makaixcom/KStart.git
cd KStart
cargo build --locked --release
```

产物为 `target/release/kstart.exe`。在项目根目录运行它，或者与 `skin/`、`assets/`、`icons/` 一起复制到发布目录。

```powershell
cargo test --locked
```

依赖已缓存时可使用 `cargo build --offline` 和 `cargo test --offline`。

## 自动发布 Windows Release

仓库内置 [Windows Release 工作流](.github/workflows/windows-release.yml)。推送以 `v` 开头的版本标签后，GitHub Actions 会在 Windows 环境运行测试、编译 x64 发行版、打包程序和资源，并将 ZIP 与 SHA-256 校验文件上传到对应 Release。

先提交并推送代码，再发布标签，例如：

```powershell
git tag v0.1.0
git push origin v0.1.0
```

发布新版本时使用新的标签；带连字符的标签（例如 `v0.2.0-beta.1`）会标记为预发布。建议同步更新 `Cargo.toml` 中的版本号并提交 `Cargo.lock`。

Release 正文由 [更新说明脚本](scripts/generate-release-notes.ps1) 自动生成，汇总最近的祖先版本标签到当前标签之间的非合并提交，并附完整比较链接。首个版本汇总全部历史。提交标题使用 `feat: 新增功能`、`fix: 修复问题`（也支持 `bugfix:`）、`perf: 性能优化`、`docs: 文档更新`，会分别归类；支持 `feat(search): 搜索功能` 等作用域。带 `!`（如 `feat!: 调整配置格式`）或正文含 `BREAKING CHANGE:` 的提交单列为不兼容变更。其他提交归入“其他更新”，不会丢失。重跑同一标签的工作流会重新生成正文并更新附件；手动修改的 Release 正文会被覆盖。

也可以在仓库 **Actions → Windows Release → Run workflow** 手动编译：选择普通分支时只生成可下载的构建产物；选择版本标签时会创建或更新该标签的 Release。工作流使用仓库自带的 `GITHUB_TOKEN`，不需要另填个人令牌；仓库或组织需允许 GitHub Actions 及相应的仓库写入权限。

发行包不会包含开发目录里的 `user/`，首次运行会生成默认数据。工作流上传前会运行 [打包脚本](scripts/package-windows.ps1)。

参考：[GitHub Actions 触发事件](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows)、[GitHub CLI 发布 Release](https://cli.github.com/manual/gh_release_create)。

## 皮肤与数据

皮肤位于 `skin/<皮肤目录名>/`，每款皮肤包含 `Setup.ini` 以及对应的位图和图标。预览图优先使用 `<皮肤目录名>.gif`，其次为 `Pre.gif`；作者、时间和描述从 `Pre.txt` 读取，兼容 UTF-8 与 GBK。

运行期数据集中保存在 `user/`：

| 文件或目录 | 内容 |
| --- | --- |
| `config.ini` | 窗口、高级设置、热键和字体等配置 |
| `data.json` | 类别、栏目及快捷方式 |
| `plan.json` | 计划任务、批处理步骤及执行记录 |

图标只保存来源路径，直接读取原文件；`.lnk` 读取目标程序图标，不向磁盘写入图标缓存。

主界面底部文件夹图标用于新建普通栏目，确认后保存并展开新栏目；文件夹/网址栏目暂不开放。栏目标题按输入原文保存、显示和编辑，保留符号及首尾空格，不自动添加装饰符号。删除栏目时需要确认，可删除最后一个栏目；空类别再次添加快捷方式时自动创建默认栏目。

计划任务由 KStart 在运行期间调度，支持每天、每周、每月、每年、准确时间、间隔、倒计时、启动/退出和网络连通状态变化；支持提醒、批处理与毫秒延时，不支持农历。间隔及倒计时的时间框表示时长（时:分:秒）。提醒内容最多 500 字，可选 60 秒自动关闭、播放声音，支持声音文件选择与试听；声音只保存来源路径。提醒窗口独立显示，不阻塞其他计划任务。

主界面搜索框使用原生文本控件，支持 Home/End、Shift 选中、Ctrl+A/C/X/V/Z 和 Delete，回车按所选模式搜索；在搜索框中按 Ctrl+A 只全选文字。

## 开发资料

- [项目指南](AGENTS.md)
- [构建与验证](.agents/skills/kstart-build-and-verify/SKILL.md)
- [皮肤编写](.agents/skills/kstart-skin-authoring/SKILL.md)
- [代码与性能维护说明](docs/optimization-notes.md)

## 致谢

感谢原版音速启动及皮肤作者。本仓库保留皮肤包自带的作者说明；其中的第三方皮肤、图标及其他素材权利归原作者所有，使用或再分发时请遵守各自授权。
