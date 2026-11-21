---
name: kstart-skin-authoring
description: 新增、迁移或排查 KStart 皮肤包时使用，覆盖 Setup.ini、八切片、控件贴图和缩略图。
---

# KStart 皮肤编写

先查阅 [UI、皮肤与数据规范](../../rules/02-ui-skin-and-data.md) 中的皮肤发现、配置兼容和坐标契约。字段解析以 `src/skin_parser.rs` 为准，资产加载以 `src/skin_assets.rs` 为准。

## 1. 核对皮肤包

每款皮肤放在 `skin/<皮肤目录名>/`，使用独立的 `Setup.ini`。加载器会检查以下贴图：

| 类型 | 文件 |
| --- | --- |
| 窗口边与角 | `Skin-1.bmp` 至 `Skin-8.bmp`，顺序为左、上、右、下、左上、右上、左下、右下 |
| 标题按钮 | `Min_Normal.bmp`、`Skin_Normal.bmp`、`Close_Normal.bmp` |
| 预览区、类别标签 | `S_Normal.bmp`、`C_Normal.ico` |
| 栏目条与菜单 | `Bar.bmp`、`MenuLeft.bmp`、`MenuCheck.ico` |

功能按钮使用 `button1_Normal.ico` 至 `button11_Normal.ico` 中需要的资源。可选的搜索按钮为 `SearchButton_Normal.bmp`，滚动资源为 `Scroll.bmp` 和 `UpDown.bmp`。支持三态的控件可附对应的 `_Move`、`_Down` 贴图。预览区只读取 `S_Normal.bmp`。作者说明和缩略图按 UI 规则中的发现顺序放置。

## 2. 配置字段速查

| 区域 | 字段 |
| --- | --- |
| 窗口、栏目和按钮文字 | `WindowMaxWidth`、`WindowMaxHeight`、`WindowBgColor`、`WindowMaskColor`、`GroupBarTextColor`、`BtnTextNormalColor`、`BtnTextHoverColor` |
| 四角 | `CornerMask{TopLeft,TopRight,BottomLeft,BottomRight}{Width,Height}` |
| 菜单 | `MenuBgColor`、`MenuBorderColor`、`MenuItemBorderColor`、`MenuSeparatorColor`、`MenuItemHoverBgColor` |
| 功能按钮 | `Btn{Status,Mail,Category,FolderLock,Backup,Schedule,MainMenu,ViewMode,ThemeBrush,Hotkey,LockScreen}{Left,Top}` |
| 日期、标题按钮 | `TimeLabel{Left,Top,Color,Bold}`、`TitleBtn{Min,Skin,Close}{Left,Top,Width,Height}` |
| 列表和图标预览 | `ItemList{Left,Top,Width,Height,BgColor}`、`PreviewIcon{Left,Top,Width,Height}` |
| 类别标签 | `CategoryTabStart{Left,Top}`、`CategoryTab{Width,Height,StepOffset,Direction}`、`CategoryTabIconOffset{X,Y}` |
| 类别标题 | `CategoryTabText{Visible,Left,Top,Width,Height,Color}` |
| 搜索框和搜索按钮 | `SearchBox{Visible,BgColor,TextColor,Left,Top,Width,Height,UseSkinBackground}`、`SearchBtn{Left,Top,Width,Height}` |

花括号表示需展开的字段后缀。例如 `CornerMaskTopLeftWidth`。`CategoryTabDirection=1` 为纵向，`2` 为横向。迁移带类别标题的皮肤时核对 `ClassShowText` 和 `ClassText*` 的兼容读取。搜索框轮廓已经画在窗口贴图里的皮肤应配置 `SearchBoxUseSkinBackground=1`。

## 3. 验证

1. 用隔离配置的 `Skin` 指向待验证皮肤，确认皮肤在列表中可发现，配置和贴图成功加载。
2. 按[构建与验证手册](../kstart-build-and-verify/SKILL.md)导出截图，并在缩小和放大后的尺寸下检查边角、透明裁剪和锚点。
3. 根据变更核对菜单配色、按钮状态、类别标题、搜索框底图和缩略图。涉及实际定位、悬停或点击时使用实机验证。
