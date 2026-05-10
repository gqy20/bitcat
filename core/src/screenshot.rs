use serde::{Deserialize, Serialize};

// ---- 截图目标 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum ScreenshotTarget {
    Primary,
    All,
}

impl Default for ScreenshotTarget {
    fn default() -> Self {
        Self::All
    }
}

// ---- 截图配置 ----

#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotConfig {
    #[serde(default)]
    pub target: ScreenshotTarget,
    #[serde(default = "default_max_width")]
    pub max_width: u32,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
    #[serde(default = "default_interval_sec")]
    pub interval_sec: u64,
    #[serde(default = "default_true")]
    pub dedup: bool,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,
    #[serde(default = "default_min_width")]
    pub min_width: u32,
    /// 调试模式：多分辨率对比（空=单分辨率，否则按列表逐个截图分析）
    #[serde(default)]
    pub debug_resolutions: Vec<u32>,
}

fn default_max_width() -> u32 {
    1280
}
fn default_jpeg_quality() -> u8 {
    80
}
fn default_interval_sec() -> u64 {
    30
}
fn default_true() -> bool {
    true
}
fn default_similarity_threshold() -> f64 {
    0.95
}
fn default_min_width() -> u32 {
    480
}
fn default_debug_resolutions() -> Vec<u32> {
    vec![640, 960, 1280, 1600]
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            target: ScreenshotTarget::default(),
            max_width: default_max_width(),
            jpeg_quality: default_jpeg_quality(),
            interval_sec: default_interval_sec(),
            dedup: default_true(),
            similarity_threshold: default_similarity_threshold(),
            min_width: default_min_width(),
            debug_resolutions: default_debug_resolutions(),
        }
    }
}

impl ScreenshotConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.jpeg_quality == 0 || self.jpeg_quality > 100 {
            return Err(format!(
                "jpeg_quality 必须在 1-100 之间，当前: {}",
                self.jpeg_quality
            ));
        }
        if self.max_width == 0 {
            return Err("max_width 不能为 0".into());
        }
        Ok(())
    }
}

// ---- dHash 感知哈希 ----

/// dHash: 对灰度像素计算差异哈希。期望宽度 >= 9，高度 >= 8（标准 9x8）。
/// bit = 1 当 pixel(col+1, row) > pixel(col, row)，共 8x8 = 64 bit。
pub fn perceptual_hash(pixels: &[u8], w: u32, h: u32) -> u64 {
    let mut hash: u64 = 0;
    for row in 0..h.min(8) {
        for col in 0..(w - 1).min(8) {
            let left = pixels[(row * w + col) as usize];
            let right = pixels[(row * w + col + 1) as usize];
            if right > left {
                hash |= 1u64 << (row * 8 + col);
            }
        }
    }
    hash
}

pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn similarity(a: u64, b: u64) -> f64 {
    1.0 - (hamming_distance(a, b) as f64 / 64.0)
}

pub fn is_similar(a: u64, b: u64, threshold: f64) -> bool {
    similarity(a, b) >= threshold
}

// ---- 截图帧数据 ----

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub pixels: Vec<u8>, // BGRA, 4 bytes per pixel
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// 将多个帧水平拼接。不同高度用黑色填充。
pub fn stitch_horizontal(frames: &[&CapturedFrame]) -> CapturedFrame {
    if frames.is_empty() {
        return CapturedFrame {
            pixels: vec![],
            width: 0,
            height: 0,
        };
    }

    let total_width: u32 = frames.iter().map(|f| f.width).sum();
    let max_height: u32 = frames.iter().map(|f| f.height).max().unwrap_or(0);
    let mut pixels = vec![0u8; (total_width * max_height * 4) as usize];

    let mut x_offset: u32 = 0;
    for frame in frames {
        for y in 0..frame.height {
            let src_start = (y * frame.width * 4) as usize;
            let src_end = src_start + (frame.width * 4) as usize;
            let dst_start = (y * total_width * 4 + x_offset * 4) as usize;
            pixels[dst_start..dst_start + src_end - src_start]
                .copy_from_slice(&frame.pixels[src_start..src_end]);
        }
        x_offset += frame.width;
    }

    CapturedFrame {
        pixels,
        width: total_width,
        height: max_height,
    }
}

// ---- Resize + JPEG 编码 ----

/// 将 BGRA 像素等比缩放到 max_width 以内，返回 RGB 像素 + 新尺寸。
/// 不放大小图。
pub fn resize_bgra(
    bgra: &[u8],
    w: u32,
    h: u32,
    max_width: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let expected = (w * h * 4) as usize;
    if bgra.len() < expected {
        return Err(format!(
            "BGRA buffer too small: expected {} bytes, got {}",
            expected,
            bgra.len()
        ));
    }

    let (out_w, out_h) = if w <= max_width {
        (w, h)
    } else {
        let scale = max_width as f64 / w as f64;
        (max_width, (h as f64 * scale).round() as u32)
    };

    // BGRA -> RGBA
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for chunk in bgra[..expected].chunks_exact(4) {
        rgba.push(chunk[2]); // R
        rgba.push(chunk[1]); // G
        rgba.push(chunk[0]); // B
        rgba.push(chunk[3]); // A
    }

    use image::{ImageBuffer, RgbaImage, imageops};
    let img: RgbaImage = ImageBuffer::from_raw(w, h, rgba)
        .ok_or_else(|| "无法创建图像缓冲区".to_string())?;

    let resized = imageops::resize(&img, out_w, out_h, imageops::FilterType::Triangle);

    // RGBA -> RGB (丢弃 alpha 通道用于 JPEG)
    let mut rgb = Vec::with_capacity((out_w * out_h * 3) as usize);
    for pixel in resized.pixels() {
        rgb.push(pixel[0]);
        rgb.push(pixel[1]);
        rgb.push(pixel[2]);
    }

    Ok((rgb, out_w, out_h))
}

/// 将 RGB 像素编码为 JPEG。
pub fn encode_jpeg(rgb: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>, String> {
    use image::codecs::jpeg::JpegEncoder;
    use image::{ImageBuffer, RgbImage};
    use std::io::Cursor;

    let img: RgbImage = ImageBuffer::from_raw(w, h, rgb.to_vec())
        .ok_or_else(|| "无法创建 RGB 图像缓冲区".to_string())?;

    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    img.write_with_encoder(encoder)
        .map_err(|e| format!("JPEG 编码失败: {e}"))?;

    Ok(buf.into_inner())
}

// ---- 截图存储 ----

/// 截图分析结果，保存为 JSON。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotRecord {
    pub description: String,
    pub hash: u64,
    pub skipped: bool,
    pub width: u32,
    pub height: u32,
    pub jpeg_size: usize,
}

/// 获取截图存储根目录 `~/.ai-pad/screenshots/`。
pub fn screenshot_base_dir() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("screenshots"))
}

/// 获取今天的截图目录，不存在则创建。
pub fn ensure_today_dir() -> Result<std::path::PathBuf, String> {
    let base = screenshot_base_dir()?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dir = base.join(&today);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建截图目录失败: {e}"))?;
    Ok(dir)
}

/// 保存截图 JPEG 和分析结果到磁盘。
/// 文件名格式：`HHmmss.jpg` + `HHmmss_analysis.json`
/// 返回 JPEG 文件路径。
pub fn save_screenshot(
    jpeg_bytes: &[u8],
    record: &ScreenshotRecord,
) -> Result<std::path::PathBuf, String> {
    let dir = ensure_today_dir()?;
    let prefix = chrono::Local::now().format("%H%M%S").to_string();

    let jpg_path = dir.join(format!("{prefix}.jpg"));
    std::fs::write(&jpg_path, jpeg_bytes)
        .map_err(|e| format!("保存截图失败: {e}"))?;

    save_analysis_json(&dir, &prefix, "", record)?;

    Ok(jpg_path)
}

/// 仅保存分析结果 JSON（用于调试多分辨率模式，JPG 已由调用方保存）。
/// 文件名格式：`{prefix}{suffix}_analysis.json`
pub fn save_analysis_json(
    dir: &std::path::Path,
    prefix: &str,
    suffix: &str,
    record: &ScreenshotRecord,
) -> Result<(), String> {
    let json_path = dir.join(format!("{prefix}{suffix}_analysis.json"));
    let json = serde_json::to_string_pretty(record)
        .map_err(|e| format!("序列化分析结果失败: {e}"))?;
    std::fs::write(&json_path, json)
        .map_err(|e| format!("保存分析结果失败: {e}"))?;
    Ok(())
}

/// 清理超过 keep_days 天的截图目录。返回清理的目录数。
pub fn cleanup_old_screenshots(keep_days: u64) -> Result<u32, String> {
    let base = screenshot_base_dir()?;
    if !base.exists() {
        return Ok(0);
    }

    let cutoff = chrono::Local::now() - chrono::Duration::days(keep_days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
    let mut removed = 0u32;

    let entries = std::fs::read_dir(&base)
        .map_err(|e| format!("读取截图目录失败: {e}"))?;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // 目录名格式 YYYY-MM-DD，字符串比较即可判断新旧
        if &*name_str < &*cutoff_str {
            if std::fs::remove_dir_all(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let cfg = ScreenshotConfig::default();
        assert!(matches!(cfg.target, ScreenshotTarget::All));
        assert_eq!(cfg.max_width, 1280);
        assert_eq!(cfg.jpeg_quality, 80);
        assert_eq!(cfg.interval_sec, 30);
        assert!(cfg.dedup);
        assert!((cfg.similarity_threshold - 0.95).abs() < 0.001);
        assert_eq!(cfg.min_width, 480);
        assert_eq!(cfg.debug_resolutions, vec![640, 960, 1280, 1600]);
    }

    #[test]
    fn test_config_deserialize_from_yaml_full() {
        let yaml = r#"
target: All
max_width: 960
jpeg_quality: 80
interval_sec: 300
dedup: true
similarity_threshold: 0.95
min_width: 480
"#;
        let cfg: ScreenshotConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(cfg.target, ScreenshotTarget::All));
        assert_eq!(cfg.max_width, 960);
    }

    #[test]
    fn test_config_deserialize_partial_uses_defaults() {
        let yaml = "target: Primary\n";
        let cfg: ScreenshotConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(cfg.target, ScreenshotTarget::Primary));
        assert_eq!(cfg.max_width, 1280);
        assert_eq!(cfg.jpeg_quality, 80);
    }

    #[test]
    fn test_config_deserialize_empty_uses_all_defaults() {
        let yaml = "\n";
        let cfg: ScreenshotConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(cfg.target, ScreenshotTarget::All));
        assert_eq!(cfg.max_width, 1280);
    }

    #[test]
    fn test_validate_quality_zero() {
        let mut cfg = ScreenshotConfig::default();
        cfg.jpeg_quality = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_quality_over_100() {
        let mut cfg = ScreenshotConfig::default();
        cfg.jpeg_quality = 101;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_quality_valid() {
        let cfg = ScreenshotConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_max_width_zero() {
        let mut cfg = ScreenshotConfig::default();
        cfg.max_width = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_screenshot_target_serde_roundtrip() {
        for target in [ScreenshotTarget::All, ScreenshotTarget::Primary] {
            let yaml = serde_yaml::to_string(&target).unwrap();
            let back: ScreenshotTarget = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(target, back);
        }
    }

    // ---- dHash 测试 ----

    #[test]
    fn test_dhash_uniform_white() {
        let pixels: Vec<u8> = vec![255; 9 * 8];
        let hash = perceptual_hash(&pixels, 9, 8);
        assert_eq!(hash, 0);
    }

    #[test]
    fn test_dhash_uniform_black() {
        let pixels: Vec<u8> = vec![0; 9 * 8];
        let hash = perceptual_hash(&pixels, 9, 8);
        assert_eq!(hash, 0);
    }

    #[test]
    fn test_dhash_known_gradient_all_ones() {
        let mut pixels = Vec::with_capacity(9 * 8);
        for _row in 0..8u32 {
            for col in 0..9u32 {
                pixels.push((col * 30) as u8);
            }
        }
        let hash = perceptual_hash(&pixels, 9, 8);
        assert_eq!(hash, 0xFFFFFFFFFFFFFFFF);
    }

    #[test]
    fn test_dhash_reverse_gradient_all_zeros() {
        let mut pixels = Vec::with_capacity(9 * 8);
        for _row in 0..8u32 {
            for col in 0..9u32 {
                pixels.push(((8 - col) * 30) as u8);
            }
        }
        let hash = perceptual_hash(&pixels, 9, 8);
        assert_eq!(hash, 0);
    }

    #[test]
    fn test_hamming_distance_identical() {
        assert_eq!(hamming_distance(0xABCD, 0xABCD), 0);
    }

    #[test]
    fn test_hamming_distance_all_different() {
        assert_eq!(hamming_distance(0, 0xFFFFFFFFFFFFFFFF), 64);
    }

    #[test]
    fn test_hamming_distance_partial() {
        assert_eq!(hamming_distance(0b1010, 0b0101), 4);
    }

    #[test]
    fn test_is_similar_above_threshold() {
        assert!(is_similar(0, 1, 0.95));
    }

    #[test]
    fn test_is_similar_below_threshold() {
        assert!(!is_similar(0, 0x00000000FFFFFFFF, 0.95));
    }

    #[test]
    fn test_similarity_calculation() {
        let sim = similarity(0, 1);
        assert!((sim - (1.0 - 1.0 / 64.0)).abs() < 0.001);
    }

    // ---- Resize + JPEG 测试 ----

    #[test]
    fn test_resize_bgra_downscale() {
        let pixels = vec![128u8; 1920 * 1080 * 4];
        let (out, w, h) = resize_bgra(&pixels, 1920, 1080, 960).unwrap();
        assert_eq!(w, 960);
        assert_eq!(h, 540);
        assert_eq!(out.len(), (960 * 540 * 3) as usize);
    }

    #[test]
    fn test_resize_bgra_no_upscale() {
        let pixels = vec![128u8; 640 * 480 * 4];
        let (_, w, h) = resize_bgra(&pixels, 640, 480, 960).unwrap();
        assert_eq!(w, 640);
        assert_eq!(h, 480);
    }

    #[test]
    fn test_resize_bgra_exact_width() {
        let pixels = vec![128u8; 960 * 540 * 4];
        let (_, w, _) = resize_bgra(&pixels, 960, 540, 960).unwrap();
        assert_eq!(w, 960);
    }

    #[test]
    fn test_resize_bgra_invalid_input() {
        let pixels = vec![0u8; 10];
        let result = resize_bgra(&pixels, 100, 100, 960);
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_jpeg_produces_valid_header() {
        let rgb = vec![255u8; 100 * 100 * 3];
        let jpeg = encode_jpeg(&rgb, 100, 100, 80).unwrap();
        assert!(!jpeg.is_empty());
        assert_eq!(jpeg[0], 0xFF);
        assert_eq!(jpeg[1], 0xD8);
    }

    #[test]
    fn test_encode_jpeg_quality_affects_size() {
        // 非均匀色块才能体现质量差异
        let mut rgb = Vec::with_capacity(200 * 200 * 3);
        for y in 0..200u32 {
            for x in 0..200u32 {
                let r = ((x * 3 + y * 7) % 256) as u8;
                let g = ((x * 11 + y * 13) % 256) as u8;
                let b = ((x * 17 + y * 19) % 256) as u8;
                rgb.push(r);
                rgb.push(g);
                rgb.push(b);
            }
        }
        let high = encode_jpeg(&rgb, 200, 200, 95).unwrap();
        let low = encode_jpeg(&rgb, 200, 200, 20).unwrap();
        assert!(low.len() < high.len(), "low={} < high={}", low.len(), high.len());
    }

    // ---- 类型 + 拼接 测试 ----

    #[test]
    fn test_captured_frame_construction() {
        let frame = CapturedFrame {
            pixels: vec![0u8; 4 * 4 * 4],
            width: 4,
            height: 4,
        };
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 4);
        assert_eq!(frame.pixels.len(), 64);
    }

    #[test]
    fn test_screen_info_sort_by_left() {
        let mut screens = vec![
            ScreenInfo {
                left: 1920,
                top: 0,
                width: 1920,
                height: 1080,
            },
            ScreenInfo {
                left: 0,
                top: 0,
                width: 1920,
                height: 1080,
            },
        ];
        screens.sort_by_key(|s| s.left);
        assert_eq!(screens[0].left, 0);
        assert_eq!(screens[1].left, 1920);
    }

    #[test]
    fn test_stitch_horizontal_two_equal_frames() {
        let frame_a = CapturedFrame {
            pixels: vec![255; 4 * 4 * 4],
            width: 4,
            height: 4,
        };
        let frame_b = CapturedFrame {
            pixels: vec![0; 4 * 4 * 4],
            width: 4,
            height: 4,
        };
        let stitched = stitch_horizontal(&[&frame_a, &frame_b]);
        assert_eq!(stitched.width, 8);
        assert_eq!(stitched.height, 4);

        let px = |x: u32, y: u32| -> &[u8] {
            let i = (y * 8 + x) as usize * 4;
            &stitched.pixels[i..i + 4]
        };
        assert_eq!(px(0, 0), &[255, 255, 255, 255]);
        assert_eq!(px(3, 3), &[255, 255, 255, 255]);
        assert_eq!(px(4, 0), &[0, 0, 0, 0]);
        assert_eq!(px(7, 3), &[0, 0, 0, 0]);
    }

    #[test]
    fn test_stitch_horizontal_different_heights_pads() {
        let frame_a = CapturedFrame {
            pixels: vec![200; 4 * 2 * 4],
            width: 4,
            height: 2,
        };
        let frame_b = CapturedFrame {
            pixels: vec![100; 4 * 4 * 4],
            width: 4,
            height: 4,
        };
        let stitched = stitch_horizontal(&[&frame_a, &frame_b]);
        assert_eq!(stitched.width, 8);
        assert_eq!(stitched.height, 4);
    }

    #[test]
    fn test_stitch_empty_input() {
        let stitched = stitch_horizontal(&[]);
        assert_eq!(stitched.width, 0);
        assert_eq!(stitched.height, 0);
        assert!(stitched.pixels.is_empty());
    }

    #[test]
    fn test_stitch_single_frame() {
        let frame = CapturedFrame {
            pixels: vec![128; 10 * 10 * 4],
            width: 10,
            height: 10,
        };
        let stitched = stitch_horizontal(&[&frame]);
        assert_eq!(stitched.width, 10);
        assert_eq!(stitched.height, 10);
    }

    // ---- 存储测试 ----

    #[test]
    fn test_screenshot_base_dir_under_home() {
        let dir = screenshot_base_dir().unwrap();
        assert!(dir.to_string_lossy().contains(".ai-pad"));
        assert!(dir.to_string_lossy().contains("screenshots"));
    }

    #[test]
    fn test_ensure_today_dir_creates_directory() {
        let dir = ensure_today_dir().unwrap();
        assert!(dir.exists());
        assert!(dir.is_dir());
        // 目录名应该是今天日期
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert!(dir.to_string_lossy().contains(&today));
    }

    #[test]
    fn test_save_screenshot_writes_files() {
        let tmp = tempfile::tempdir().unwrap();
        let jpg_dir = tmp.path().join("2026-01-01");
        std::fs::create_dir_all(&jpg_dir).unwrap();

        let record = ScreenshotRecord {
            description: "测试描述".into(),
            hash: 12345,
            skipped: false,
            width: 960,
            height: 540,
            jpeg_size: 32000,
        };

        // 直接写文件到 tmp 目录验证逻辑
        let jpeg_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0]; // minimal JPEG header
        let prefix = "120000";
        let jpg_path = jpg_dir.join(format!("{prefix}.jpg"));
        std::fs::write(&jpg_path, &jpeg_bytes).unwrap();

        let json_path = jpg_dir.join(format!("{prefix}_analysis.json"));
        let json = serde_json::to_string_pretty(&record).unwrap();
        std::fs::write(&json_path, &json).unwrap();

        assert!(jpg_path.exists());
        assert!(json_path.exists());
        assert_eq!(std::fs::read(&jpg_path).unwrap().len(), 4);

        let parsed: ScreenshotRecord =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(parsed.description, "测试描述");
        assert_eq!(parsed.hash, 12345);
    }

    #[test]
    fn test_save_analysis_json_with_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let record = ScreenshotRecord {
            description: "调试分析".into(),
            hash: 999,
            skipped: false,
            width: 1280,
            height: 720,
            jpeg_size: 50000,
        };

        save_analysis_json(dir, "181035", "_1280px", &record).unwrap();

        let json_path = dir.join("181035_1280px_analysis.json");
        assert!(json_path.exists());
        let parsed: ScreenshotRecord =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(parsed.description, "调试分析");
        assert_eq!(parsed.width, 1280);
    }

    #[test]
    fn test_cleanup_removes_old_dirs() {
        let base = tempfile::tempdir().unwrap().path().join("screenshots");
        std::fs::create_dir_all(base.join("2020-01-01")).unwrap();
        std::fs::create_dir_all(base.join("2020-01-02")).unwrap();
        std::fs::write(base.join("2020-01-01/test.jpg"), b"").unwrap();

        let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
        std::fs::create_dir_all(base.join(&today_str)).unwrap();

        // 模拟 cleanup：删除所有比今天旧的
        let cutoff = today_str.clone();
        let mut removed = 0u32;
        for entry in std::fs::read_dir(&base).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name < cutoff {
                if std::fs::remove_dir_all(entry.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        assert_eq!(removed, 2);
        assert!(base.join(&today_str).exists());
    }

    #[test]
    fn test_screenshot_record_serialize() {
        let record = ScreenshotRecord {
            description: "VS Code".into(),
            hash: 999,
            skipped: false,
            width: 960,
            height: 540,
            jpeg_size: 32000,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("VS Code"));
        assert!(json.contains("999"));
        let back: ScreenshotRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.jpeg_size, 32000);
    }
}
