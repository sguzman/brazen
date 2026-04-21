#[cfg(feature = "servo-upstream")]
mod tests {
    use brazen::engine::{AlphaMode, ColorSpace, EngineFrame, PixelFormat};
    use brazen::rendering::{normalize_pixels, probe_frame_stats};
    use serial_test::serial;
    use brazen::servo_upstream::{ServoUpstreamConfig, ServoUpstreamRuntime};
    use brazen::mounts::MountManager;
    use brazen::permissions::PermissionPolicy;

    #[test]
    #[serial]
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
        let session = std::sync::Arc::new(std::sync::RwLock::new(brazen::session::SessionSnapshot::new("default".to_string(), "now".to_string())));
        let mut runtime = ServoUpstreamRuntime::new(96, 96, config, tx, mount_manager, permissions, session).expect("servo runtime");
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
            if snapshot.url.contains("data") {
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
