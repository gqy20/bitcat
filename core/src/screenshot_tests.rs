#[cfg(test)]
mod tests {
    use crate::screenshot::*;

    #[test]
    fn test_default_debug_resolutions_is_empty() {
        let cfg = ScreenshotConfig::default();
        assert!(
            cfg.debug_resolutions.is_empty(),
            "生产默认不应启用调试多分辨率，当前 {:?}",
            cfg.debug_resolutions
        );
    }

    #[test]
    fn test_default_config_values() {
        let cfg = ScreenshotConfig::default();
        assert!(matches!(cfg.target, ScreenshotTarget::All));
        assert_eq!(cfg.max_width, 960);
        assert_eq!(cfg.jpeg_quality, 80);
        assert_eq!(cfg.interval_sec, 30);
        assert!(cfg.dedup);
        assert!((cfg.similarity_threshold - 0.95).abs() < 0.001);
        assert_eq!(cfg.min_width, 480);
        assert!(cfg.debug_resolutions.is_empty());
    }

    #[test]
    fn test_from_env_overrides_max_width() {
        unsafe { std::env::set_var("SCREENSHOT_MAX_WIDTH", "640") };
        let cfg = ScreenshotConfig::from_env();
        assert_eq!(cfg.max_width, 640);
        unsafe { std::env::remove_var("SCREENSHOT_MAX_WIDTH") };

        let cfg = ScreenshotConfig::from_env();
        assert_eq!(cfg.max_width, 960);
    }

    #[test]
    fn test_from_env_ignores_invalid_value() {
        unsafe { std::env::set_var("SCREENSHOT_MAX_WIDTH", "not_a_number") };
        let cfg = ScreenshotConfig::from_env();
        assert_eq!(cfg.max_width, 960);
        unsafe { std::env::remove_var("SCREENSHOT_MAX_WIDTH") };
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
        assert_eq!(cfg.max_width, 960);
        assert_eq!(cfg.jpeg_quality, 80);
    }

    #[test]
    fn test_config_deserialize_empty_uses_all_defaults() {
        let yaml = "\n";
        let cfg: ScreenshotConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(cfg.target, ScreenshotTarget::All));
        assert_eq!(cfg.max_width, 960);
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
        assert!(
            low.len() < high.len(),
            "low={} < high={}",
            low.len(),
            high.len()
        );
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

        let jpeg_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let prefix = "120000";
        let jpg_path = jpg_dir.join(format!("{prefix}.jpg"));
        std::fs::write(&jpg_path, &jpeg_bytes).unwrap();

        let json_path = jpg_dir.join(format!("{prefix}_analysis.json"));
        let json = serde_json::to_string_pretty(&record).unwrap();
        std::fs::write(&json_path, json).unwrap();

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
    fn test_screenshot_record_snapshot() {
        let record = ScreenshotRecord {
            description: "VS Code".into(),
            hash: 999,
            skipped: false,
            width: 960,
            height: 540,
            jpeg_size: 32000,
        };
        insta::assert_yaml_snapshot!(record);
    }

    // ---- list_recent_analyses 测试 ----

    #[test]
    fn test_list_recent_analyses_returns_records_in_time_order() {
        let dir = tempfile::tempdir().unwrap();
        let today = dir.path().join("2026-05-11");
        std::fs::create_dir_all(&today).unwrap();

        for (i, desc) in ["浏览器", "终端", "VS Code"].iter().enumerate() {
            let prefix = format!("{:06}", 100000 + i * 10);
            let record = ScreenshotRecord {
                description: (*desc).into(),
                hash: i as u64,
                skipped: false,
                width: 1280,
                height: 800,
                jpeg_size: 5000 + i * 1000,
            };
            save_analysis_json(&today, &prefix, "", &record).unwrap();
        }

        let results = list_recent_analyses(today.as_path(), 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].description, "VS Code");
        assert_eq!(results[1].description, "终端");
        assert_eq!(results[2].description, "浏览器");
    }

    #[test]
    fn test_list_recent_analyses_respects_count_limit() {
        let dir = tempfile::tempdir().unwrap();
        let today = dir.path().join("2026-05-11");
        std::fs::create_dir_all(&today).unwrap();

        for i in 0..5 {
            let prefix = format!("{:06}", 100000 + i * 10);
            let record = ScreenshotRecord {
                description: format!("截图{}", i),
                hash: i as u64,
                skipped: false,
                width: 1280,
                height: 800,
                jpeg_size: 5000,
            };
            save_analysis_json(&today, &prefix, "", &record).unwrap();
        }

        let results = list_recent_analyses(today.as_path(), 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].description, "截图4");
        assert_eq!(results[1].description, "截图3");
    }

    #[test]
    fn test_list_recent_analyses_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty-day");
        std::fs::create_dir_all(&empty).unwrap();

        let results = list_recent_analyses(empty.as_path(), 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_list_recent_analyses_skips_non_json_files() {
        let dir = tempfile::tempdir().unwrap();
        let today = dir.path().join("2026-05-11");
        std::fs::create_dir_all(&today).unwrap();

        let record = ScreenshotRecord {
            description: "有效".into(),
            hash: 1,
            skipped: false,
            width: 1280,
            height: 800,
            jpeg_size: 5000,
        };
        save_analysis_json(&today, "120000", "", &record).unwrap();
        std::fs::write(today.join("120001.jpg"), b"fake").unwrap();
        std::fs::write(today.join("README.txt"), b"notes").unwrap();

        let results = list_recent_analyses(today.as_path(), 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].description, "有效");
    }

    // ---- list_recent_analyses_multi_day 测试 ----

    #[test]
    fn test_list_recent_multi_day_single_dir() {
        let base = tempfile::tempdir().unwrap();
        let day1 = base.path().join("2026-05-11");
        std::fs::create_dir_all(&day1).unwrap();

        for i in 0..3 {
            let prefix = format!("{:06}", 100000 + i * 10);
            save_analysis_json(
                &day1,
                &prefix,
                "",
                &ScreenshotRecord {
                    description: format!("截图{}", i),
                    hash: i as u64,
                    skipped: false,
                    width: 1280,
                    height: 800,
                    jpeg_size: 5000,
                },
            )
            .unwrap();
        }

        let results = list_recent_analyses_multi_day(base.path(), 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].1.description, "截图2");
        assert_eq!(results[2].1.description, "截图0");
    }

    #[test]
    fn test_list_recent_multi_day_cross_day() {
        let base = tempfile::tempdir().unwrap();
        let day1 = base.path().join("2026-05-10");
        let day2 = base.path().join("2026-05-11");
        std::fs::create_dir_all(&day1).unwrap();
        std::fs::create_dir_all(&day2).unwrap();

        save_analysis_json(
            &day2,
            "150000",
            "",
            &ScreenshotRecord {
                description: "今天".into(),
                hash: 1,
                skipped: false,
                width: 1280,
                height: 800,
                jpeg_size: 5000,
            },
        )
        .unwrap();

        for i in 0..3 {
            let prefix = format!("{:06}", 100000 + i * 10);
            save_analysis_json(
                &day1,
                &prefix,
                "",
                &ScreenshotRecord {
                    description: format!("昨天{}", i),
                    hash: i as u64,
                    skipped: false,
                    width: 1280,
                    height: 800,
                    jpeg_size: 5000,
                },
            )
            .unwrap();
        }

        let results = list_recent_analyses_multi_day(base.path(), 4);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].1.description, "今天");
        assert_eq!(results[1].1.description, "昨天2");
        assert_eq!(results[2].1.description, "昨天1");
        assert_eq!(results[3].1.description, "昨天0");
    }

    #[test]
    fn test_list_recent_multi_day_empty_base() {
        let base = tempfile::tempdir().unwrap();
        let results = list_recent_analyses_multi_day(base.path(), 10);
        assert!(results.is_empty());
    }

    // ---- build_recent_analyses_context 测试 ----

    #[test]
    fn test_build_recent_context_format() {
        let base = tempfile::tempdir().unwrap();
        let day = base.path().join("2026-05-11");
        std::fs::create_dir_all(&day).unwrap();

        save_analysis_json(
            &day,
            "120000",
            "",
            &ScreenshotRecord {
                description: "用户在写代码".into(),
                hash: 1,
                skipped: false,
                width: 1280,
                height: 800,
                jpeg_size: 5000,
            },
        )
        .unwrap();

        let ctx = build_recent_analyses_context_with_base(10, 1500, base.path());
        assert!(ctx.starts_with("[最近截图观察]\n"));
        assert!(ctx.contains("用户在写代码"));
        assert!(ctx.ends_with("[/最近截图观察]\n"));
    }

    #[test]
    fn test_build_recent_context_empty() {
        let base = tempfile::tempdir().unwrap();
        let ctx = build_recent_analyses_context_with_base(10, 1500, base.path());
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_build_recent_context_truncation() {
        let base = tempfile::tempdir().unwrap();
        let day = base.path().join("2026-05-11");
        std::fs::create_dir_all(&day).unwrap();

        for i in 0..20 {
            let prefix = format!("{:06}", 100000 + i);
            save_analysis_json(
                &day,
                &prefix,
                "",
                &ScreenshotRecord {
                    description: "这是一条很长的截图分析描述用于测试截断功能是否正常工作".into(),
                    hash: i as u64,
                    skipped: false,
                    width: 1280,
                    height: 800,
                    jpeg_size: 5000,
                },
            )
            .unwrap();
        }

        let ctx = build_recent_analyses_context_with_base(20, 200, base.path());
        assert!(
            ctx.chars().count() <= 250,
            "应在 {} 字符内",
            ctx.chars().count()
        );
    }

    #[test]
    fn test_build_recent_context_order_old_to_new() {
        let base = tempfile::tempdir().unwrap();
        let day = base.path().join("2026-05-11");
        std::fs::create_dir_all(&day).unwrap();

        for (i, desc) in ["最早", "中间", "最新"].iter().enumerate() {
            let prefix = format!("{:06}", 100000 + i * 100);
            save_analysis_json(
                &day,
                &prefix,
                "",
                &ScreenshotRecord {
                    description: (*desc).into(),
                    hash: i as u64,
                    skipped: false,
                    width: 1280,
                    height: 800,
                    jpeg_size: 5000,
                },
            )
            .unwrap();
        }

        let ctx = build_recent_analyses_context_with_base(10, 1500, base.path());
        let earliest = ctx.find("最早").unwrap();
        let middle = ctx.find("中间").unwrap();
        let latest = ctx.find("最新").unwrap();
        assert!(earliest < middle && middle < latest, "应从旧到新排列");
    }
}
