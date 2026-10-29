//! CSS [`cursor`](https://developer.mozilla.org/zh-CN/docs/Web/CSS/Reference/Properties/cursor)
//! 关键字（Level 4 预定义值）与 `url()` 自定义图像列表。

use winit::cursor::CursorIcon;

/// 一条 `url(...)` 图像，可选热点（CSS 像素，图像左上为原点）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorUrl {
    pub src: String,
    pub hotspot: Option<(i32, i32)>,
}

/// Full `cursor` value: `[ url(...) [x y]? , ]* <keyword>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorValue {
    pub images: Vec<CursorUrl>,
    pub kind: CursorKind,
}

impl CursorValue {
    pub fn keyword(kind: CursorKind) -> Self {
        Self {
            images: Vec::new(),
            kind,
        }
    }

    /// `inherit` / `unset` / empty → [`None`](Option::None) (inherit ancestor).
    /// `initial` → `auto`.
    ///
    /// 不以逗号切分整串（`data:image/png;base64,...` 内部有逗号）。
    pub fn parse_css(raw: &str) -> Option<Self> {
        let mut rest = raw.trim();
        if rest.is_empty()
            || rest.eq_ignore_ascii_case("inherit")
            || rest.eq_ignore_ascii_case("unset")
        {
            return None;
        }
        if rest.eq_ignore_ascii_case("initial") {
            return Some(Self::keyword(CursorKind::Auto));
        }

        let mut images = Vec::new();
        loop {
            rest = rest.trim_start();
            let Some((src, after)) = take_url(rest) else {
                break;
            };
            let after = after.trim_start();
            let (hotspot, after) = take_hotspot(after);
            images.push(CursorUrl { src, hotspot });
            rest = after.trim_start();
            if let Some(stripped) = rest.strip_prefix(',') {
                rest = stripped.trim_start();
                continue;
            }
            break;
        }

        let kind = if rest.is_empty() {
            CursorKind::Auto
        } else {
            let token = rest.split_whitespace().next().unwrap_or(rest);
            CursorKind::parse_keyword(token).unwrap_or(CursorKind::Auto)
        };
        Some(Self { images, kind })
    }
}

impl From<CursorKind> for CursorValue {
    fn from(kind: CursorKind) -> Self {
        Self::keyword(kind)
    }
}

fn take_url(input: &str) -> Option<(String, &str)> {
    let t = input.trim_start();
    if t.len() < 4 || !t.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("url(")) {
        return None;
    }
    let after_kw = t[4..].trim_start();
    if after_kw.starts_with('"') || after_kw.starts_with('\'') {
        let quote = after_kw.as_bytes()[0];
        let inner = &after_kw[1..];
        let end = inner.find(|c| c == quote as char)?;
        let src = inner[..end].to_string();
        let after_q = inner[end + 1..].trim_start();
        let after_q = after_q.strip_prefix(')')?;
        Some((src, after_q))
    } else {
        let end = after_kw.find(')')?;
        let src = after_kw[..end].trim().to_string();
        Some((src, &after_kw[end + 1..]))
    }
}

fn take_hotspot(input: &str) -> (Option<(i32, i32)>, &str) {
    let (x, rest) = match take_css_int(input) {
        Some(v) => v,
        None => return (None, input),
    };
    let rest = rest.trim_start();
    let Some((y, rest)) = take_css_int(rest) else {
        return (None, input);
    };
    (Some((x, y)), rest)
}

fn take_css_int(input: &str) -> Option<(i32, &str)> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }
    let mut end = 0;
    let bytes = s.as_bytes();
    if bytes[0] == b'+' || bytes[0] == b'-' {
        end = 1;
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == digits_start {
        return None;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        let frac = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == frac {
            return None;
        }
    }
    let num: f64 = s[..end].parse().ok()?;
    if !num.is_finite() || num < i32::MIN as f64 || num > i32::MAX as f64 {
        return None;
    }
    let mut rest = &s[end..];
    if rest.len() >= 2 && rest[..2].eq_ignore_ascii_case("px") {
        rest = &rest[2..];
    }
    if !rest.is_empty() {
        let c = rest.as_bytes()[0];
        if c.is_ascii_alphabetic() || c == b'-' || c == b'_' {
            return None;
        }
    }
    Some((num as i32, rest))
}

/// CSS `cursor` 预定义关键字。
///
/// `auto` 在应用到窗口时等价于 `default`（箭头）；可编辑文本由控件显式设 `text`。
/// `none` 隐藏指针（`Window::set_cursor_visible(false)`），winit 无对应 `CursorIcon`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Auto,
    Default,
    None,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
    AllScroll,
    ZoomIn,
    ZoomOut,
}

impl CursorKind {
    /// 解析 CSS `cursor` 声明的关键字后备（忽略 `url()` 是否能加载）。
    pub fn parse_css(raw: &str) -> Option<Self> {
        CursorValue::parse_css(raw).map(|v| v.kind)
    }

    pub fn parse_keyword(s: &str) -> Option<Self> {
        Some(
            match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
                "auto" => Self::Auto,
                "default" => Self::Default,
                "none" => Self::None,
                "context-menu" => Self::ContextMenu,
                "help" => Self::Help,
                "pointer" => Self::Pointer,
                "progress" => Self::Progress,
                "wait" => Self::Wait,
                "cell" => Self::Cell,
                "crosshair" => Self::Crosshair,
                "text" => Self::Text,
                "vertical-text" => Self::VerticalText,
                "alias" => Self::Alias,
                "copy" => Self::Copy,
                "move" => Self::Move,
                "no-drop" => Self::NoDrop,
                "not-allowed" => Self::NotAllowed,
                "grab" => Self::Grab,
                "grabbing" => Self::Grabbing,
                "e-resize" => Self::EResize,
                "n-resize" => Self::NResize,
                "ne-resize" => Self::NeResize,
                "nw-resize" => Self::NwResize,
                "s-resize" => Self::SResize,
                "se-resize" => Self::SeResize,
                "sw-resize" => Self::SwResize,
                "w-resize" => Self::WResize,
                "ew-resize" => Self::EwResize,
                "ns-resize" => Self::NsResize,
                "nesw-resize" => Self::NeswResize,
                "nwse-resize" => Self::NwseResize,
                "col-resize" => Self::ColResize,
                "row-resize" => Self::RowResize,
                "all-scroll" => Self::AllScroll,
                "zoom-in" => Self::ZoomIn,
                "zoom-out" => Self::ZoomOut,
                _ => return None,
            },
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Default => "default",
            Self::None => "none",
            Self::ContextMenu => "context-menu",
            Self::Help => "help",
            Self::Pointer => "pointer",
            Self::Progress => "progress",
            Self::Wait => "wait",
            Self::Cell => "cell",
            Self::Crosshair => "crosshair",
            Self::Text => "text",
            Self::VerticalText => "vertical-text",
            Self::Alias => "alias",
            Self::Copy => "copy",
            Self::Move => "move",
            Self::NoDrop => "no-drop",
            Self::NotAllowed => "not-allowed",
            Self::Grab => "grab",
            Self::Grabbing => "grabbing",
            Self::EResize => "e-resize",
            Self::NResize => "n-resize",
            Self::NeResize => "ne-resize",
            Self::NwResize => "nw-resize",
            Self::SResize => "s-resize",
            Self::SeResize => "se-resize",
            Self::SwResize => "sw-resize",
            Self::WResize => "w-resize",
            Self::EwResize => "ew-resize",
            Self::NsResize => "ns-resize",
            Self::NeswResize => "nesw-resize",
            Self::NwseResize => "nwse-resize",
            Self::ColResize => "col-resize",
            Self::RowResize => "row-resize",
            Self::AllScroll => "all-scroll",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
        }
    }

    /// `auto` → `default`。`none` 无图标。
    pub fn to_winit(self) -> Option<CursorIcon> {
        Some(match self {
            Self::Auto | Self::Default => CursorIcon::Default,
            Self::None => return None,
            Self::ContextMenu => CursorIcon::ContextMenu,
            Self::Help => CursorIcon::Help,
            Self::Pointer => CursorIcon::Pointer,
            Self::Progress => CursorIcon::Progress,
            Self::Wait => CursorIcon::Wait,
            Self::Cell => CursorIcon::Cell,
            Self::Crosshair => CursorIcon::Crosshair,
            Self::Text => CursorIcon::Text,
            Self::VerticalText => CursorIcon::VerticalText,
            Self::Alias => CursorIcon::Alias,
            Self::Copy => CursorIcon::Copy,
            Self::Move => CursorIcon::Move,
            Self::NoDrop => CursorIcon::NoDrop,
            Self::NotAllowed => CursorIcon::NotAllowed,
            Self::Grab => CursorIcon::Grab,
            Self::Grabbing => CursorIcon::Grabbing,
            Self::EResize => CursorIcon::EResize,
            Self::NResize => CursorIcon::NResize,
            Self::NeResize => CursorIcon::NeResize,
            Self::NwResize => CursorIcon::NwResize,
            Self::SResize => CursorIcon::SResize,
            Self::SeResize => CursorIcon::SeResize,
            Self::SwResize => CursorIcon::SwResize,
            Self::WResize => CursorIcon::WResize,
            Self::EwResize => CursorIcon::EwResize,
            Self::NsResize => CursorIcon::NsResize,
            Self::NeswResize => CursorIcon::NeswResize,
            Self::NwseResize => CursorIcon::NwseResize,
            Self::ColResize => CursorIcon::ColResize,
            Self::RowResize => CursorIcon::RowResize,
            Self::AllScroll => CursorIcon::AllScroll,
            Self::ZoomIn => CursorIcon::ZoomIn,
            Self::ZoomOut => CursorIcon::ZoomOut,
        })
    }

    pub fn apply_to_window(self, window: &dyn winit::window::Window) {
        match self.to_winit() {
            None => window.set_cursor_visible(false),
            Some(icon) => {
                window.set_cursor_visible(true);
                window.set_cursor(winit::cursor::Cursor::Icon(icon));
            }
        }
    }

    /// MDN 表格中的全部预定义关键字（含 `auto`），供 Gallery / 测试枚举。
    pub const ALL_KEYWORDS: &'static [&'static str] = &[
        "auto",
        "default",
        "none",
        "context-menu",
        "help",
        "pointer",
        "progress",
        "wait",
        "cell",
        "crosshair",
        "text",
        "vertical-text",
        "alias",
        "copy",
        "move",
        "no-drop",
        "not-allowed",
        "grab",
        "grabbing",
        "e-resize",
        "n-resize",
        "ne-resize",
        "nw-resize",
        "s-resize",
        "se-resize",
        "sw-resize",
        "w-resize",
        "ew-resize",
        "ns-resize",
        "nesw-resize",
        "nwse-resize",
        "col-resize",
        "row-resize",
        "all-scroll",
        "zoom-in",
        "zoom-out",
    ];
}

#[cfg(test)]
#[path = "tests/cursor.rs"]
mod tests;
