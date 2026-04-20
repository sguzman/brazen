#[cfg(feature = "servo-upstream")]
mod tests {
    use brazen::engine::{AlphaMode, ColorSpace, EngineFrame, PixelFormat};
    use brazen::rendering::{normalize_pixels, probe_frame_stats};
    use brazen::servo_upstream::{ServoUpstreamConfig, ServoUpstreamRuntime};
    use brazen::mounts::MountManager;
    use brazen::permissions::PermissionPolicy;

    fn render_about_blank_frame() -> EngineFrame {
        let config = ServoUpstreamConfig {
            pixel_format: PixelFormat::Rgba8,
            alpha_mode: AlphaMode::Straight,
            color_space: ColorSpace::Srgb,
            enable_pixel_probe: false,
            resources_dir: None,
            certificate_path: None,
            ignore_certificate_errors: false,
        };
        let (tx, _) = std::sync::mpsc::channel();
        let mount_manager = MountManager::new();
        let permissions = PermissionPolicy::default();
        let mut runtime = ServoUpstreamRuntime::new(64, 64, config, tx, mount_manager, permissions).expect("servo runtime");
        for _ in 0..40 {
            runtime.spin();
            if let Some(frame) = runtime.render_frame() {
                return EngineFrame {
                    width: frame.width,
                    height: frame.height,
                    frame_number: 1,
                    stride_bytes: frame.stride_bytes,
                    pixel_format: frame.pixel_format,
                    alpha_mode: frame.alpha_mode,
                    color_space: frame.color_space,
                    pixels: frame.pixels,
                };
            }
        }
        panic!("no frame captured for about:blank");
    }

    fn sample_color(pixels: &[u8], width: usize, x: usize, y: usize) -> (u8, u8, u8, u8) {
        let idx = (y * width + x) * 4;
        (
            pixels[idx],
            pixels[idx + 1],
            pixels[idx + 2],
            pixels[idx + 3],
        )
    }

    #[test]
    fn about_blank_is_visually_uniform() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let frame = render_about_blank_frame();
        let pixels = normalize_pixels(&frame, false);
        assert!(!pixels.is_empty());
        let width = frame.width as usize;
        let height = frame.height as usize;
        let a = sample_color(&pixels, width, 0, 0);
        let b = sample_color(&pixels, width, width / 2, height / 2);
        let c = sample_color(&pixels, width, width - 1, height - 1);
        for (left, right) in [(a, b), (a, c), (b, c)] {
            let diff = (
                left.0.abs_diff(right.0),
                left.1.abs_diff(right.1),
                left.2.abs_diff(right.2),
            );
            assert!(diff.0 <= 8 && diff.1 <= 8 && diff.2 <= 8);
        }
        assert!(a.3 >= 240);
    }

    #[test]
    fn data_url_renders_non_white_pixels() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = ServoUpstreamConfig {
            pixel_format: PixelFormat::Rgba8,
            alpha_mode: AlphaMode::Straight,
            color_space: ColorSpace::Srgb,
            enable_pixel_probe: false,
            resources_dir: None,
            certificate_path: None,
            ignore_certificate_errors: false,
        };
        let (tx, _) = std::sync::mpsc::channel();
        let mount_manager = MountManager::new();
        let permissions = PermissionPolicy::default();
        let mut runtime = ServoUpstreamRuntime::new(96, 96, config, tx, mount_manager, permissions).expect("servo runtime");
        loop {
            runtime.spin();
            if runtime.snapshot().load_status == libservo::LoadStatus::Complete {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let mut retry_count = 0;
        loop {
            runtime.navigate("data:text/html,<html><body style='background:blue;margin:0'></body></html>").ok();
            runtime.spin();
            if runtime.snapshot().url.contains("background:blue") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            if retry_count > 50 { 
                panic!("failed to navigate after 50 retries");
            }
            retry_count += 1;
        }

        let mut frames_captured = 0;
        for i in 0..1000 {
            runtime.spin();
            
            let snapshot = runtime.snapshot();
            if snapshot.url.contains("data") && !load_finished {
                 // Wait for load to at least start or finish
                 // LoadStatus::Finished is what we want
            }

            if let Some(frame) = runtime.render_frame() {
                frames_captured += 1;
                let pixels = normalize_pixels(
                    &EngineFrame {
                        width: frame.width,
                        height: frame.height,
                        frame_number: i as u64,
                        stride_bytes: frame.stride_bytes,
                        pixel_format: frame.pixel_format,
                        alpha_mode: frame.alpha_mode,
                        color_space: frame.color_space,
                        pixels: frame.pixels,
                    },
                    false,
                );
                
                if let Some(stats) = probe_frame_stats(&pixels, frame.width, frame.height, 128) {
                    if stats.non_white_ratio > 0.01 {
                        return;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("data url never produced non-white pixels after {} frames (final url: {}, load status: {:?})", 
            frames_captured, runtime.snapshot().url, runtime.snapshot().load_status);
    }
}
