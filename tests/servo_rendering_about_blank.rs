#[cfg(feature = "servo-upstream")]
mod tests {
    use brazen::engine::{AlphaMode, ColorSpace, EngineFrame, PixelFormat};
    use brazen::rendering::{normalize_pixels, probe_frame_stats};
    use serial_test::serial;
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
        let session = std::sync::Arc::new(std::sync::RwLock::new(brazen::session::SessionSnapshot::new("default".to_string(), "now".to_string())));
        let mut runtime = ServoUpstreamRuntime::new(64, 64, config, tx, mount_manager, permissions, session).expect("servo runtime");
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
    #[serial]
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

}
