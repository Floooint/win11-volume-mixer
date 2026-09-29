//! 托盘音量图标：用系统图标字体绘制，与 Windows 自带的音量图标一致。

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FW_NORMAL, GdiFlush,
    GetTextFaceW, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::SM_CXSMICON;
use windows_core::HSTRING;

use crate::audio::VolumeState;

/// Windows 11 的图标字体；Windows 10 上回退为 Segoe MDL2 Assets，两者这些字符的编码相同。
const FONTS: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];

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

/// 托盘小图标的边长（物理像素），随系统缩放变化：100% 为 16，150% 为 24。
pub fn icon_size() -> u32 {
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    if size > 0 { size as u32 } else { 16 }
}

/// 绘制图标，返回 RGBA 像素。`light` 为浅色任务栏，此时图标为黑色，否则为白色。
pub fn render(level: Level, size: u32, light: bool) -> Option<Vec<u8>> {
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
        let mut font = None;
        for name in FONTS {
            let candidate = CreateFontW(
                -side,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                0,
                &HSTRING::from(name),
            );
            let previous = SelectObject(dc, HGDIOBJ(candidate.0));
            let mut face = [0u16; 64];
            let len = GetTextFaceW(dc, Some(&mut face)).max(1) as usize - 1;
            // 字体不存在时 GDI 会静默替换成其他字体，需要确认实际选中的字体名。
            if String::from_utf16_lossy(&face[..len]).eq_ignore_ascii_case(name) {
                font = Some((candidate, previous));
                break;
            }
            SelectObject(dc, previous);
            let _ = DeleteObject(HGDIOBJ(candidate.0));
        }

        let mut pixels = None;
        if let Some((font, previous)) = font {
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            SetBkMode(dc, TRANSPARENT);
            let mut text: Vec<u16> = level.glyph().encode_utf16(&mut [0; 2]).to_vec();
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
            pixels = Some(to_rgba(bgra, light));
            SelectObject(dc, previous);
            let _ = DeleteObject(HGDIOBJ(font.0));
        }

        SelectObject(dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(dc);
        pixels
    }
}

/// 黑底白字的灰度图转为单色 RGBA：亮度作为 alpha。
fn to_rgba(bgra: &[u8], light: bool) -> Vec<u8> {
    let color = if light { 0 } else { 255 };
    bgra.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [color, color, color, p[1]])
        .collect()
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
        assert_eq!(to_rgba(&[200, 200, 200, 0], false), [255, 255, 255, 200]);
        assert_eq!(to_rgba(&[200, 200, 200, 0], true), [0, 0, 0, 200]);
    }

    #[test]
    fn 能绘制所有图标() {
        for level in [
            Level::Muted,
            Level::Zero,
            Level::Low,
            Level::Medium,
            Level::High,
        ] {
            let pixels = render(level, 24, false).expect("系统缺少图标字体");
            assert_eq!(pixels.len(), 24 * 24 * 4);
            assert!(
                pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
                "{level:?} 为空白"
            );
        }
    }
}
