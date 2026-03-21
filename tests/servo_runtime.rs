#[cfg(feature = "servo")]
mod tests {
    use brazen::config::EngineConfig;
    use brazen::engine::RenderSurfaceMetadata;
    use brazen::servo_embedder::{ServoEmbedder, ServoEmbedderConfig};

    #[test]
    fn servo_embedder_renders_frame_after_surface_attach() {
        let config = ServoEmbedderConfig::from_engine_config(&EngineConfig::default());
        let mut embedder = ServoEmbedder::new(config);
        embedder.init().unwrap();
        embedder.attach_surface(
            brazen::engine::RenderSurfaceHandle {
                id: 1,
                label: "test".to_string(),
            },
            RenderSurfaceMetadata {
                viewport_width: 64,
                viewport_height: 64,
                scale_factor_basis_points: 100,
            },
        );
        let frame = embedder.render_frame();
        assert!(frame.is_some());
        let frame = frame.unwrap();
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert_eq!(frame.pixels.len(), 64 * 64 * 4);
    }
}
