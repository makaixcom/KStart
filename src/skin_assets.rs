//! 皮肤贴图资源加载、系统图标提取与 2D 像素级画布渲染引擎
//! 支持 BMP / ICO / GIF / PNG 加载，以及从外部拖入的文件/快捷方式提取系统图标。

use crate::skin_parser::{SkinConfig, ToolButtonKind};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub use crate::surface::RgbaSurface;
pub use crate::canvas::Canvas;
pub use crate::file_icons::{extract_file_icon_32, load_custom_icon_16};

/// 按钮三态交互状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetState {
    Normal,
    Hover,
    Down,
}

/// 三态图像资源（常态、悬停、按下）
#[derive(Debug, Clone)]
pub struct TriStateImage {
    pub normal: RgbaSurface,
    pub hover: Option<RgbaSurface>,
    pub down: Option<RgbaSurface>,
}

impl TriStateImage {
    /// 根据前缀名称加载三态图像
    pub fn load(skin_dir: &Path, prefix: &str, ext: &str) -> Option<Self> {
        let normal = RgbaSurface::load_file(&skin_dir.join(format!("{}_Normal.{}", prefix, ext)))?;
        let hover = RgbaSurface::load_file(&skin_dir.join(format!("{}_Move.{}", prefix, ext)));
        let down = RgbaSurface::load_file(&skin_dir.join(format!("{}_Down.{}", prefix, ext)));
        Some(Self {
            normal,
            hover,
            down,
        })
    }

    /// 根据当前状态获取对应图像表面
    pub fn get(&self, state: WidgetState) -> &RgbaSurface {
        match state {
            WidgetState::Normal => &self.normal,
            WidgetState::Hover => self.hover.as_ref().unwrap_or(&self.normal),
            WidgetState::Down => self
                .down
                .as_ref()
                .or(self.hover.as_ref())
                .unwrap_or(&self.normal),
        }
    }
}

/// 皮肤完整图像资源集合
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SkinAssets {
    pub skin_left: RgbaSurface,
    pub skin_top: RgbaSurface,
    pub skin_right: RgbaSurface,
    pub skin_bottom: RgbaSurface,
    pub skin_top_left: RgbaSurface,
    pub skin_top_right: RgbaSurface,
    pub skin_bottom_left: RgbaSurface,
    pub skin_bottom_right: RgbaSurface,

    pub btn_min: TriStateImage,
    pub btn_skin: TriStateImage,
    pub btn_close: TriStateImage,

    pub preview_base: RgbaSurface,
    pub category_tab: TriStateImage,
    pub search_btn: Option<TriStateImage>,

    pub group_bar: RgbaSurface,
    pub scroll_thumb: Option<RgbaSurface>,
    pub up_down_arrows: Option<RgbaSurface>,

    pub menu_left_strip: RgbaSurface,
    pub menu_check: RgbaSurface,

    pub tool_buttons: HashMap<ToolButtonKind, TriStateImage>,
    pub preview_thumbnail: Option<RgbaSurface>,
}

impl SkinAssets {
    /// 从皮肤目录加载所有贴图资源
    pub fn load(skin_dir: &Path, cfg: &SkinConfig) -> Result<Self, String> {
        let load_req = |name: &str| -> Result<RgbaSurface, String> {
            RgbaSurface::load_file(&skin_dir.join(name))
                .ok_or_else(|| format!("缺少皮肤贴图文件: {}", name))
        };

        let skin_left = load_req("Skin-1.bmp")?;
        let skin_top = load_req("Skin-2.bmp")?;
        let skin_right = load_req("Skin-3.bmp")?;
        let skin_bottom = load_req("Skin-4.bmp")?;
        let mut skin_top_left = load_req("Skin-5.bmp")?;
        let mut skin_top_right = load_req("Skin-6.bmp")?;
        let mut skin_bottom_left = load_req("Skin-7.bmp")?;
        let mut skin_bottom_right = load_req("Skin-8.bmp")?;

        let mc = cfg.window_mask_color;
        let cm = &cfg.corner_mask;
        skin_top_left.apply_color_key_in_rect(0, 0, cm.top_left_width, cm.top_left_height, mc);

        let tr_x = skin_top_right.width.saturating_sub(cm.top_right_width);
        skin_top_right.apply_color_key_in_rect(
            tr_x,
            0,
            cm.top_right_width,
            cm.top_right_height,
            mc,
        );

        let bl_y = skin_bottom_left.height.saturating_sub(cm.bottom_left_height);
        skin_bottom_left.apply_color_key_in_rect(
            0,
            bl_y,
            cm.bottom_left_width,
            cm.bottom_left_height,
            mc,
        );

        let br_x = skin_bottom_right
            .width
            .saturating_sub(cm.bottom_right_width);
        let br_y = skin_bottom_right
            .height
            .saturating_sub(cm.bottom_right_height);
        skin_bottom_right.apply_color_key_in_rect(
            br_x,
            br_y,
            cm.bottom_right_width,
            cm.bottom_right_height,
            mc,
        );

        let btn_min = TriStateImage::load(skin_dir, "Min", "bmp")
            .ok_or_else(|| "缺少 Min_Normal.bmp".to_string())?;
        let btn_skin = TriStateImage::load(skin_dir, "Skin", "bmp")
            .ok_or_else(|| "缺少 Skin_Normal.bmp".to_string())?;
        let btn_close = TriStateImage::load(skin_dir, "Close", "bmp")
            .ok_or_else(|| "缺少 Close_Normal.bmp".to_string())?;

        let preview_base = load_req("S_Normal.bmp")?;
        let category_tab = TriStateImage::load(skin_dir, "C", "ico")
            .ok_or_else(|| "缺少 C_Normal.ico".to_string())?;
        let search_btn = TriStateImage::load(skin_dir, "SearchButton", "bmp");

        let group_bar = load_req("Bar.bmp")?;
        let scroll_thumb = RgbaSurface::load_file(&skin_dir.join("Scroll.bmp"));
        let up_down_arrows = RgbaSurface::load_file(&skin_dir.join("UpDown.bmp"));
        let menu_left_strip = load_req("MenuLeft.bmp")?;
        let menu_check = load_req("MenuCheck.ico")?;

        let mut tool_buttons = HashMap::new();
        for kind in ToolButtonKind::ALL {
            let (_, _, idx) = kind.config_keys();
            let prefix = format!("button{}", idx);
            if let Some(tri) = TriStateImage::load(skin_dir, &prefix, "ico") {
                tool_buttons.insert(kind, tri);
            }
        }

        let folder_name = skin_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let preview_thumbnail =
            RgbaSurface::load_file(&skin_dir.join(format!("{}.gif", folder_name)))
                .or_else(|| RgbaSurface::load_file(&skin_dir.join("Pre.gif")));

        Ok(Self {
            skin_left,
            skin_top,
            skin_right,
            skin_bottom,
            skin_top_left,
            skin_top_right,
            skin_bottom_left,
            skin_bottom_right,
            btn_min,
            btn_skin,
            btn_close,
            preview_base,
            category_tab,
            search_btn,
            group_bar,
            scroll_thumb,
            up_down_arrows,
            menu_left_strip,
            menu_check,
            tool_buttons,
            preview_thumbnail,
        })
    }
}

/// 内置与用户自定义图标资源库
#[derive(Debug, Clone)]
pub struct BuiltinIcons {
    pub folder_large: RgbaSurface,
    pub cat_laptop: RgbaSurface,
    pub cat_cube: RgbaSurface,
    pub group_search: RgbaSurface,
    pub menu_options: RgbaSurface,
    pub menu_hotkey: RgbaSurface,
    pub menu_schedule: RgbaSurface,
    pub menu_mail: RgbaSurface,
    pub menu_folder: RgbaSurface,
    pub menu_skin: RgbaSurface,
    pub menu_exit: RgbaSurface,
    pub ctx_add_file: RgbaSurface,
    pub ctx_add_dir: RgbaSurface,
    pub ctx_add_url: RgbaSurface,
    pub ctx_add_sys: RgbaSurface,
    pub ctx_add_login: RgbaSurface,
    pub ctx_search: RgbaSurface,
    pub ctx_appearance: RgbaSurface,
    pub ctx_paste: RgbaSurface,
    pub ctx_clear_col: RgbaSurface,
    pub ctx_lock_col: RgbaSurface,
    pub ctx_del_col: RgbaSurface,
    pub im_delete: RgbaSurface,
    pub im_cut: RgbaSurface,
    pub im_copy: RgbaSurface,
    pub im_schedule: RgbaSurface,
    pub im_move_top: RgbaSurface,
    pub im_move_bottom: RgbaSurface,
    pub im_edit: RgbaSurface,
    pub im_props: RgbaSurface,
    pub about_banner: RgbaSurface,
    pub app_icons: HashMap<String, RgbaSurface>,
}

impl BuiltinIcons {
    pub fn category_icon(&self, name: &str) -> &RgbaSurface {
        let key = match name {
            "cat_laptop" => "icons/small/Laptop.ico",
            "cat_cube" => "icons/small/工程.ico",
            _ => name,
        };
        self.app_icons.get(key).unwrap_or_else(|| if name.contains("cube") { &self.cat_cube } else { &self.cat_laptop })
    }
    /// 加载 `assets/icons/` 与 `user/icons/` 下的所有像素图标
    pub fn load(base_dir: &Path) -> Self {
        let icon_dir = base_dir.join("assets").join("icons");
        let load_or_empty = |name: &str, w: u32, h: u32| -> RgbaSurface {
            RgbaSurface::load_file(&icon_dir.join(format!("{}.png", name)))
                .unwrap_or_else(|| RgbaSurface::new_empty(w, h))
        };
        let about_banner = RgbaSurface::load_file(&base_dir.join("assets").join("about_banner.png"))
            .unwrap_or_else(|| RgbaSurface::new_empty(137, 360));

        let mut app_icons = HashMap::new();
        for key in [
            "app_msedge",
            "app_7zfm",
            "app_erp",
            "app_everything",
            "app_yuque",
            "app_univpn",
            "app_wechat",
            "app_prevent",
            "app_clash",
            "app_vscode",
            "app_obsidian",
            "app_notepadpp",
            "app_hoppscotch",
            "app_pixpin",
            "app_tabby",
            "app_meatshell",
            "app_fiddler",
            "app_navicat",
            "app_myhash64",
            "app_typora",
            "app_proxylane64",
        ] {
            app_icons.insert(key.to_string(), load_or_empty(key, 16, 16));
        }

        // 加载用户拖入文件生成的自定义图标 (`user/icons/*.png`)
        let user_icon_dir = base_dir.join("user").join("icons");
        if let Ok(entries) = fs::read_dir(&user_icon_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "png").unwrap_or(false) {
                    if let Some(stem) = p.file_stem().map(|s| s.to_string_lossy().to_string()) {
                        if let Some(surf) = RgbaSurface::load_file(&p) {
                            app_icons.insert(stem, surf);
                        }
                    }
                }
            }
        }

        if let Ok(entries) = fs::read_dir(base_dir.join("icons/small")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(icon) = RgbaSurface::load_file(&path) {
                        let key = format!("icons/small/{}", entry.file_name().to_string_lossy());
                        app_icons.insert(key, icon.resize_to(16, 16));
                    }
                }
            }
        }
        Self {
            folder_large: load_or_empty("folder_large", 32, 32),
            cat_laptop: load_or_empty("cat_laptop", 16, 16),
            cat_cube: load_or_empty("cat_cube", 16, 16),
            group_search: load_or_empty("group_search", 16, 16),
            menu_options: load_or_empty("menu_options", 16, 16),
            menu_hotkey: load_or_empty("menu_hotkey", 16, 16),
            menu_schedule: load_or_empty("menu_schedule", 16, 16),
            menu_mail: load_or_empty("menu_mail", 16, 16),
            menu_folder: load_or_empty("menu_folder", 16, 16),
            menu_skin: load_or_empty("menu_skin", 16, 16),
            menu_exit: load_or_empty("menu_exit", 16, 16),
            ctx_add_file: load_or_empty("ctx_add_file", 16, 16),
            ctx_add_dir: load_or_empty("ctx_add_dir", 16, 16),
            ctx_add_url: load_or_empty("ctx_add_url", 16, 16),
            ctx_add_sys: load_or_empty("ctx_add_sys", 16, 16),
            ctx_add_login: load_or_empty("ctx_add_login", 16, 16),
            ctx_search: load_or_empty("ctx_search", 16, 16),
            ctx_appearance: load_or_empty("ctx_appearance", 16, 16),
            ctx_paste: load_or_empty("ctx_paste", 16, 16),
            ctx_clear_col: load_or_empty("ctx_clear_col", 16, 16),
            ctx_lock_col: load_or_empty("ctx_lock_col", 16, 16),
            ctx_del_col: load_or_empty("ctx_del_col", 16, 16),
            im_delete: load_or_empty("im_delete", 16, 16),
            im_cut: load_or_empty("im_cut", 16, 16),
            im_copy: load_or_empty("im_copy", 16, 16),
            im_schedule: load_or_empty("im_schedule", 16, 16),
            im_move_top: load_or_empty("im_move_top", 16, 16),
            im_move_bottom: load_or_empty("im_move_bottom", 16, 16),
            im_edit: load_or_empty("im_edit", 16, 16),
            im_props: load_or_empty("im_props", 16, 16),
            about_banner,
            app_icons,
        }
    }
}
