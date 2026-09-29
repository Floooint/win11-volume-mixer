//! 托盘音量图标：用系统图标字体绘制。默认样式与 Windows 自带的音量图标一致，
//! 另有耳机、音符和数字（显示音量）三种样式，颜色可自定义（设置项 `trayStyle` / `trayColor`）。

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS,
    DT_CALCRECT, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
    DrawTextW, FW_NORMAL, FW_SEMIBOLD, GdiFlush, GetTextFaceW, HDC, HFONT, HGDIOBJ,
    OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::SM_CXSMICON;
use windows_core::HSTRING;

use crate::audio::VolumeState;
use crate::config::TrayStyle;

/// Windows 11 的图标字体；Windows 10 上回退为 Segoe MDL2 Assets，两者这些字符的编码相同。
const FONTS: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];
/// 数字样式的字体。
const NUMBER_FONT: [&str; 1] = ["Segoe UI"];
/// 耳机、音符样式的字符。
const HEADPHONES: char = '\u{E7F6}';
const NOTE: char = '\u{EC4F}';
/// 静音时淡化显示的不透明度（0–255）。
const DIM_OPACITY: u32 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    Muted,
    Zero,
    Low,
    Medium,
    High,
}

impl Level {
    /// 分档与系统托盘音量图标一致：0、1–33、34–66、67–100。
    pub fn of(volume: Option<VolumeState>) -> Self {
        let Some(volume) = volume else {
            return Self::Muted;
        };
        if volume.muted {
            return Self::Muted;
        }
        match (volume.volume * 100.0).round() as u32 {
            0 => Self::Zero,
            1..=33 => Self::Low,
            34..=66 => Self::Medium,
            _ => Self::High,
        }
    }

    fn glyph(self) -> char {
        match self {
            Self::Muted => '\u{E74F}',
            Self::Zero => '\u{E992}',
            Self::Low => '\u{E993}',
            Self::Medium => '\u{E994}',
            Self::High => '\u{E995}',
        }
    }
}

/// 要绘制的图标：内容，以及是否淡化（耳机、音符没有静音字符，静音时淡化显示）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Icon {
    pub content: Content,
    pub dim: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Content {
    /// 图标字体中的一个字符。
    Glyph(char),
    /// 一段文字：数字样式显示的音量。
    Text(String),
}

impl Icon {
    pub fn of(style: TrayStyle, volume: Option<VolumeState>) -> Self {
        let level = Level::of(volume);
        let muted = level == Level::Muted;
        let (content, dim) = match style {
            TrayStyle::Speaker => (Content::Glyph(level.glyph()), false),
            TrayStyle::Headphones => (Content::Glyph(HEADPHONES), muted),
            TrayStyle::Note => (Content::Glyph(NOTE), muted),
            TrayStyle::Number => match volume {
                Some(v) if !muted => (
                    Content::Text(((v.volume * 100.0).round() as u32).to_string()),
                    false,
                ),
                _ => (Content::Glyph(Level::Muted.glyph()), false),
            },
        };
        Self { content, dim }
    }
}

/// 托盘小图标的边长（物理像素），随系统缩放变化：100% 为 16，150% 为 24。
pub fn icon_size() -> u32 {
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    if size > 0 { size as u32 } else { 16 }
}

/// 绘制图标，返回 RGBA 像素。`color` 为图标颜色（RGB）。
pub fn render(icon: &Icon, size: u32, color: [u8; 3]) -> Option<Vec<u8>> {
    let side = size as i32;
    let header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: side,
            biHeight: -side,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let (mut text, fonts, height, weight): (Vec<u16>, &[&str], i32, i32) = match &icon.content {
        Content::Glyph(c) => (
            c.encode_utf16(&mut [0; 2]).to_vec(),
            &FONTS,
            -side,
            FW_NORMAL.0 as i32,
        ),
        // 负值表示按字符高度（不含行距）取字号；三位数（100）字号小一些才能放下。
        Content::Text(t) => (
            t.encode_utf16().collect(),
            &NUMBER_FONT,
            -(side * if t.len() >= 3 { 9 } else { 12 } / 16),
            FW_SEMIBOLD.0 as i32,
        ),
    };

    unsafe {
        let dc = CreateCompatibleDC(None);
        let mut bits: *mut c_void = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &header, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(dc);
            return None;
        };
        let old_bitmap = SelectObject(dc, HGDIOBJ(bitmap.0));

        // 黑底白字绘制，再以亮度作为 alpha。灰度抗锯齿（而非 ClearType）保证三个通道相同。
        let mut pixels = None;
        // 字符按字号铺满图标即可；文字需要确认宽度放得下。
        let selected = match icon.content {
            Content::Glyph(_) => select_font(dc, fonts, height, 0, weight),
            Content::Text(_) => fit_font(dc, fonts, height, weight, &mut text, side),
        };
        if let Some((font, previous)) = selected {
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            SetBkMode(dc, TRANSPARENT);
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: side,
                bottom: side,
            };
            DrawTextW(
                dc,
                &mut text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            let _ = GdiFlush();
            // SAFETY: DIB 为 size × size 的 32 位位图，GdiFlush 后绘制已完成。
            let bgra = std::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize);
            let opacity = if icon.dim { DIM_OPACITY } else { 255 };
            pixels = Some(to_rgba(bgra, color, opacity));
            SelectObject(dc, previous);
            let _ = DeleteObject(HGDIOBJ(font.0));
        }

        SelectObject(dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(dc);
        pixels
    }
}

/// 选入字体；文字比图标宽时（如数字样式的“100”）逐步收窄字宽，直到能完整放下。
/// 图标字体的字符本身带有留白，测得的宽度总是等于字号，不能用这种方式处理。
///
/// # Safety
/// 同 [`select_font`]。
unsafe fn fit_font(
    dc: HDC,
    fonts: &[&str],
    height: i32,
    weight: i32,
    text: &mut [u16],
    side: i32,
) -> Option<(HFONT, HGDIOBJ)> {
    let measure = |text: &mut [u16]| {
        let mut rect = RECT::default();
        unsafe {
            DrawTextW(
                dc,
                text,
                &mut rect,
                DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
            )
        };
        rect.right - rect.left
    };
    let (mut font, mut previous) = unsafe { select_font(dc, fonts, height, 0, weight)? };
    // 0 表示按字体默认的宽高比；从约半个字高开始收窄。
    let mut width = height.abs() / 2;
    // 留出少许边距：抗锯齿的边缘会略超出测得的宽度（16 px 图标留 1 px，24 px 留 2 px）。
    while width > 1 && measure(text) > side - side / 12 {
        unsafe {
            SelectObject(dc, previous);
            let _ = DeleteObject(HGDIOBJ(font.0));
            (font, previous) = select_font(dc, fonts, height, width, weight)?;
        }
        width -= 1;
    }
    Some((font, previous))
}

/// 依次尝试字体，选入第一个系统中存在的，返回（字体，原来选入的对象）。
/// `width` 为平均字宽，0 表示按默认宽高比。调用方用完后把原对象选回并删除字体。
///
/// # Safety
/// `dc` 必须是有效的设备上下文。
unsafe fn select_font(
    dc: HDC,
    fonts: &[&str],
    height: i32,
    width: i32,
    weight: i32,
) -> Option<(HFONT, HGDIOBJ)> {
    for name in fonts {
        unsafe {
            let candidate = CreateFontW(
                height,
                width,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                0,
                &HSTRING::from(*name),
            );
            let previous = SelectObject(dc, HGDIOBJ(candidate.0));
            let mut face = [0u16; 64];
            let len = GetTextFaceW(dc, Some(&mut face)).max(1) as usize - 1;
            // 字体不存在时 GDI 会静默替换成其他字体，需要确认实际选中的字体名。
            if String::from_utf16_lossy(&face[..len]).eq_ignore_ascii_case(name) {
                return Some((candidate, previous));
            }
            SelectObject(dc, previous);
            let _ = DeleteObject(HGDIOBJ(candidate.0));
        }
    }
    None
}

/// 黑底白字的灰度图转为单色 RGBA：亮度作为 alpha，再乘以整体不透明度 `opacity`（0–255）。
fn to_rgba(bgra: &[u8], [r, g, b]: [u8; 3], opacity: u32) -> Vec<u8> {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [r, g, b, (p[1] as u32 * opacity / 255) as u8])
        .collect()
}

/// 解析 `#RRGGBB`，格式不对时为 `None`。
pub fn parse_color(color: &str) -> Option<[u8; 3]> {
    let hex = color.strip_prefix('#').filter(|h| h.len() == 6)?;
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vol(volume: f32, muted: bool) -> Option<VolumeState> {
        Some(VolumeState { volume, muted })
    }

    #[test]
    fn 音量分档与系统一致() {
        assert_eq!(Level::of(None), Level::Muted);
        assert_eq!(Level::of(vol(0.8, true)), Level::Muted);
        assert_eq!(Level::of(vol(0.0, false)), Level::Zero);
        assert_eq!(Level::of(vol(0.01, false)), Level::Low);
        assert_eq!(Level::of(vol(0.33, false)), Level::Low);
        assert_eq!(Level::of(vol(0.34, false)), Level::Medium);
        assert_eq!(Level::of(vol(0.66, false)), Level::Medium);
        assert_eq!(Level::of(vol(0.67, false)), Level::High);
        assert_eq!(Level::of(vol(1.0, false)), Level::High);
    }

    #[test]
    fn 亮度转为_alpha() {
        assert_eq!(
            to_rgba(&[200, 200, 200, 0], [255; 3], 255),
            [255, 255, 255, 200]
        );
        assert_eq!(to_rgba(&[200, 200, 200, 0], [1, 2, 3], 255), [1, 2, 3, 200]);
        assert_eq!(to_rgba(&[255, 255, 255, 0], [0; 3], 90), [0, 0, 0, 90]);
    }

    #[test]
    fn 解析十六进制颜色() {
        assert_eq!(parse_color("#FF8000"), Some([255, 128, 0]));
        assert_eq!(parse_color("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_color("FF8000"), None);
        assert_eq!(parse_color("#FF80"), None);
        assert_eq!(parse_color("#GG8000"), None);
    }

    #[test]
    fn 各样式的图标内容() {
        let icon = Icon::of(TrayStyle::Number, vol(0.426, false));
        assert_eq!(icon.content, Content::Text("43".into()));
        assert_eq!(
            Icon::of(TrayStyle::Number, vol(0.426, true)).content,
            Content::Glyph(Level::Muted.glyph()),
            "数字样式静音时显示静音图标"
        );
        assert!(
            Icon::of(TrayStyle::Headphones, vol(0.5, true)).dim,
            "静音时淡化"
        );
        assert!(Icon::of(TrayStyle::Note, None).dim, "没有设备时按静音处理");
        assert!(!Icon::of(TrayStyle::Note, vol(0.5, false)).dim);
        assert_eq!(
            Icon::of(TrayStyle::Speaker, vol(0.5, false)).content,
            Content::Glyph(Level::Medium.glyph())
        );
    }

    #[test]
    fn 能绘制所有图标() {
        let mut icons: Vec<Icon> = [
            Level::Muted,
            Level::Zero,
            Level::Low,
            Level::Medium,
            Level::High,
        ]
        .map(|level| Icon {
            content: Content::Glyph(level.glyph()),
            dim: false,
        })
        .into();
        for content in [
            Content::Glyph(HEADPHONES),
            Content::Glyph(NOTE),
            Content::Text("100".into()),
            Content::Text("7".into()),
        ] {
            icons.push(Icon {
                content,
                dim: false,
            });
        }
        for icon in icons {
            let pixels = render(&icon, 24, [255; 3]).expect("系统缺少字体");
            assert_eq!(pixels.len(), 24 * 24 * 4);
            assert!(
                pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
                "{icon:?} 为空白"
            );
        }
    }
}
