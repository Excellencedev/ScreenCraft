//! Pick an image and store it the way 0.3 did: a PNG data URL inside the project file, shrunk so
//! the project stays small.

use base64::Engine;

pub fn pick_image_data_url(max_width: u32) -> Result<Option<String>, String> {
    let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff"]).pick_file() else {
        return Ok(None);
    };
    let img = image::open(&path).map_err(|e| format!("Could not open that image: {e}"))?;
    let img = if img.width() > max_width {
        let h = (img.height() as f64 * max_width as f64 / img.width() as f64).round().max(1.0) as u32;
        img.resize_exact(max_width, h, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
}

/// Decode a `data:image/...;base64,` URL to RGBA pixels.
pub fn decode_data_url(url: &str) -> Option<(u32, u32, Vec<u8>)> {
    let b64 = url.split_once("base64,")?.1;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some((img.width(), img.height(), img.into_raw()))
}
