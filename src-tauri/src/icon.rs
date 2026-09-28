//! 应用图标：通过 Shell 提取 exe / UWP 应用图标，编码为 PNG 并缓存，
//! 经自定义协议 `appicon` 提供给前端（前端用 `convertFileSrc(source, "appicon")` 生成地址）。
//!
//! 图标来源由 `audio::app_info` 决定，见 docs/architecture.md“应用图标”。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;

use tauri::http::{Response, StatusCode, header};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, HGDIOBJ, ReleaseDC,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows_core::HSTRING;

pub const SCHEME: &str = "appicon";

/// 提取的边长（物理像素）。界面中图标约 28 逻辑像素，64 足够 200% 缩放下清晰显示。
const ICON_SIZE: i32 = 64;

type Png = Option<Arc<[u8]>>;
type Request = (String, Box<dyn FnOnce(Png) + Send>);

/// 图标线程句柄。Shell 对象要求 STA，因此单独开一个线程，不占用音频线程。
pub struct IconService {
    tx: Sender<Request>,
}

impl IconService {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        thread::Builder::new()
            .name("icon".into())
            .spawn(move || {
                if let Err(e) = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok() {
                    eprintln!("[icon] COM 初始化失败：{e}");
                    return;
                }
                // 失败结果也缓存，避免每次打开窗口都重复提取。应用数量有限，不做淘汰。
                let mut cache: HashMap<String, Png> = HashMap::new();
                for (source, reply) in rx {
                    let png = cache
                        .entry(source)
                        .or_insert_with_key(|source| match extract(source) {
                            Ok(png) => Some(png.into()),
                            Err(e) => {
                                eprintln!("[icon] 无法提取 {source} 的图标：{e}");
                                None
                            }
                        })
                        .clone();
                    reply(png);
                }
                unsafe { CoUninitialize() };
            })
            .expect("无法创建图标线程");
        Self { tx }
    }

    /// 处理 `appicon` 协议请求，路径为百分号编码的图标来源。
    pub fn respond(&self, path: &str, reply: impl FnOnce(Response<Vec<u8>>) + Send + 'static) {
        let source = percent_encoding::percent_decode_str(path.trim_start_matches('/'))
            .decode_utf8_lossy()
            .into_owned();
        let request: Request = (source, Box::new(move |png| reply(response(png))));
        if let Err(mpsc::SendError((_, reply))) = self.tx.send(request) {
            reply(None);
        }
    }
}

fn response(png: Png) -> Response<Vec<u8>> {
    let builder = Response::builder();
    match png {
        Some(png) => builder
            .header(header::CONTENT_TYPE, "image/png")
            // 同一来源的图标在程序运行期间不变。
            .header(header::CACHE_CONTROL, "max-age=86400")
            .body(png.to_vec()),
        None => builder.status(StatusCode::NOT_FOUND).body(Vec::new()),
    }
    .expect("图标响应头无效")
}

fn extract(source: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(&HSTRING::from(source), None)? };
    let size = SIZE {
        cx: ICON_SIZE,
        cy: ICON_SIZE,
    };
    let bitmap = unsafe { factory.GetImage(size, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)? };
    let pixels = read_bitmap(bitmap);
    // 位图由调用方负责释放。
    let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
    let (width, height, mut pixels) = pixels.ok_or("无法读取位图像素")?;
    bgra_to_rgba(&mut pixels);
    encode_png(width, height, &pixels)
}

/// 读取 32 位 BGRA 像素，行序为自上而下。
fn read_bitmap(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = BITMAP::default();
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            size_of::<BITMAP>() as i32,
            Some(&mut info as *mut _ as *mut _),
        )
    };
    if got == 0 || info.bmWidth <= 0 || info.bmHeight == 0 {
        return None;
    }
    let (width, height) = (info.bmWidth, info.bmHeight.abs());

    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // 负数表示自上而下，与 PNG 行序一致。
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let hdc = unsafe { GetDC(None) };
    // SAFETY: 缓冲区大小与 header 描述的 32 位、width × height 位图一致。
    let lines = unsafe {
        GetDIBits(
            hdc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut header,
            DIB_RGB_COLORS,
        )
    };
    unsafe { ReleaseDC(None, hdc) };
    (lines == height).then_some((width as u32, height as u32, pixels))
}

/// BGRA 转为 PNG 需要的非预乘 RGBA。
///
/// Shell 返回的位图通常是预乘 alpha，但不同来源不完全一致：只有所有像素的颜色分量
/// 都不超过 alpha（预乘数据必然满足）时才还原预乘；alpha 全为 0 说明位图不带透明通道，视为不透明。
fn bgra_to_rgba(pixels: &mut [u8]) {
    let (pixels, _) = pixels.as_chunks_mut::<4>();
    let no_alpha = pixels.iter().all(|p| p[3] == 0);
    let premultiplied = !no_alpha
        && pixels
            .iter()
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);

    for p in pixels {
        p.swap(0, 2);
        if no_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] > 0 && p[3] < 255 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a) as u8;
            }
        }
    }
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 预乘数据还原为非预乘并交换红蓝() {
        // BGRA：半透明红色（预乘后 R=128）和完全透明像素。
        let mut pixels = vec![0, 0, 128, 128, 0, 0, 0, 0];
        bgra_to_rgba(&mut pixels);
        assert_eq!(pixels, [255, 0, 0, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn 颜色超过_alpha_时视为非预乘不做还原() {
        let mut pixels = vec![10, 20, 200, 100];
        bgra_to_rgba(&mut pixels);
        assert_eq!(pixels, [200, 20, 10, 100]);
    }

    #[test]
    fn alpha_全为零时视为不透明() {
        let mut pixels = vec![1, 2, 3, 0, 4, 5, 6, 0];
        bgra_to_rgba(&mut pixels);
        assert_eq!(pixels, [3, 2, 1, 255, 6, 5, 4, 255]);
    }

    #[test]
    fn 编码结果是_png() {
        let png = encode_png(1, 1, &[255, 0, 0, 255]).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
