use super::*;

impl BrazenApp {
    pub(super) fn update_render_frame(&mut self, ctx: &eframe::egui::Context) {
        let Some(frame) = self.engine.render_frame() else {
            return;
        };
        self.render_frame_format = Some((frame.pixel_format, frame.alpha_mode, frame.color_space));
        if let Some(surface) = &self.last_surface
            && (surface.viewport_width != frame.width || surface.viewport_height != frame.height)
        {
            tracing::warn!(
                target: "brazen::render",
                expected_width = surface.viewport_width,
                expected_height = surface.viewport_height,
                frame_width = frame.width,
                frame_height = frame.height,
                "frame size differs from render surface"
            );
        }
        let pixels = normalize_pixels(&frame, self.config.engine.debug_bypass_swizzle);
        if pixels.is_empty() {
            return;
        }
        self.last_frame_pixels = Some(pixels.clone());
        if self.frame_probe_enabled() {
            self.frame_probe = probe_frame_stats(&pixels, frame.width, frame.height, 256);
            if let Some(stats) = self.frame_probe {
                if stats.non_white_ratio < 0.01 {
                    self.blank_frame_streak = self.blank_frame_streak.saturating_add(1);
                    if self.blank_frame_streak >= 30
                        && !self.blank_frame_warned
                        && self.shell_state.load_progress > 0.0
                    {
                        self.blank_frame_warned = true;
                        if !self.render_capture_logged {
                            let (r, g, b, a) =
                                Self::sample_pixel_rgba(&pixels, frame.width, frame.height);
                            tracing::warn!(
                                target: "brazen::render",
                                ratio = stats.non_white_ratio,
                                samples = stats.sample_count,
                                alpha_min = stats.alpha_min,
                                alpha_avg = stats.alpha_avg,
                                sample = format!("{r},{g},{b},{a}"),
                                "render capture still blank after navigation"
                            );
                            self.render_capture_logged = true;
                        }
                        tracing::warn!(
                            target: "brazen::render",
                            ratio = stats.non_white_ratio,
                            samples = stats.sample_count,
                            "render probe detected mostly white frames after navigation"
                        );
                        self.shell_state
                            .record_event("render probe: mostly white frames after navigation");
                    }
                } else {
                    self.blank_frame_streak = 0;
                    self.blank_frame_warned = false;
                    if !self.render_capture_logged {
                        tracing::info!(
                            target: "brazen::render",
                            ratio = stats.non_white_ratio,
                            samples = stats.sample_count,
                            alpha_min = stats.alpha_min,
                            alpha_avg = stats.alpha_avg,
                            "render capture detected non-white content"
                        );
                        self.render_capture_logged = true;
                    }
                }
            }
        } else {
            self.frame_probe = None;
        }
        let size = [frame.width as usize, frame.height as usize];
        let image = match frame.alpha_mode {
            AlphaMode::Premultiplied => {
                eframe::egui::ColorImage::from_rgba_premultiplied(size, &pixels)
            }
            AlphaMode::Straight => eframe::egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
        };
        let options = eframe::egui::TextureOptions::LINEAR;
        let upload_start = Instant::now();
        tracing::trace!(
            target: "brazen::render",
            frame_number = frame.frame_number,
            width = frame.width,
            height = frame.height,
            bytes = pixels.len(),
            alpha_mode = frame.alpha_mode.as_str(),
            pixel_format = frame.pixel_format.as_str(),
            color_space = frame.color_space.as_str(),
            "uploading frame to egui"
        );
        match self.render_texture.as_mut() {
            Some(texture) => {
                if texture.size() != size {
                    *texture = ctx.load_texture("brazen-render", image, options);
                } else {
                    texture.set(image, options);
                }
            }
            None => {
                self.render_texture = Some(ctx.load_texture("brazen-render", image, options));
            }
        }
        self.render_frame_number = Some(frame.frame_number);
        self.render_frame_size = Some((frame.width, frame.height));
        let upload_ms = upload_start.elapsed().as_secs_f32() * 1000.0;
        self.last_upload_ms = Some(upload_ms);
        if self.upload_times.len() == 120 {
            self.upload_times.pop_front();
        }
        self.upload_times.push_back(upload_ms);
        let now = Instant::now();
        if let Some(previous) = self.last_frame_instant {
            let ms = (now - previous).as_secs_f32() * 1000.0;
            self.last_frame_ms = Some(ms);
            if self.frame_times.len() == 120 {
                self.frame_times.pop_front();
            }
            self.frame_times.push_back(ms);
        }
        self.last_frame_instant = Some(now);
        if self.capture_next_frame {
            self.capture_next_frame = false;
            self.capture_frame_to_disk(&frame, &pixels);
        }
        match self.config.engine.frame_pacing.as_str() {
            "manual" => ctx.request_repaint_after(Duration::from_millis(16)),
            "on-demand" => {}
            _ => ctx.request_repaint(),
        }
    }

    pub(super) fn capture_frame_to_disk(&self, frame: &crate::engine::EngineFrame, pixels: &[u8]) {
        #[cfg(not(feature = "servo-upstream"))]
        let _ = pixels;
        let dir = self.resolve_capture_dir();
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::warn!(
                target: "brazen::render",
                path = %dir.display(),
                %error,
                "failed to create capture directory"
            );
            return;
        }
        let filename = format!(
            "brazen-frame-{}-{}x{}.png",
            frame.frame_number, frame.width, frame.height
        );
        let path = dir.join(filename);
        #[cfg(feature = "servo-upstream")]
        {
            let image = libservo::RgbaImage::from_raw(frame.width, frame.height, pixels.to_vec());
            match image {
                Some(image) => {
                    if let Err(error) = image.save(&path) {
                        tracing::warn!(
                            target: "brazen::render",
                            path = %path.display(),
                            %error,
                            "failed to write frame capture"
                        );
                    } else {
                        tracing::info!(
                            target: "brazen::render",
                            path = %path.display(),
                            "saved frame capture"
                        );
                    }
                }
                None => tracing::warn!(
                    target: "brazen::render",
                    path = %path.display(),
                    "failed to build capture image"
                ),
            }
        }
        #[cfg(not(feature = "servo-upstream"))]
        {
            tracing::warn!(
                target: "brazen::render",
                path = %path.display(),
                "frame capture requires the servo-upstream feature"
            );
        }
    }

    pub(super) fn save_snapshot_to_disk(&mut self) {
        let Some(pixels) = self.last_frame_pixels.as_ref() else {
            self.shell_state
                .record_event("snapshot save skipped: no frame");
            return;
        };
        let Some((width, height)) = self.render_frame_size else {
            self.shell_state
                .record_event("snapshot save skipped: no frame size");
            return;
        };
        let dir = &self.shell_state.runtime_paths.downloads_dir;
        if let Err(error) = std::fs::create_dir_all(dir) {
            tracing::warn!(
                target: "brazen::render",
                path = %dir.display(),
                %error,
                "failed to create downloads directory"
            );
            return;
        }
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let filename = format!("brazen-snapshot-{timestamp}.ppm");
        let path = dir.join(filename);
        let mut buffer = Vec::with_capacity(pixels.len() + 64);
        buffer.extend_from_slice(format!("P6\n{} {}\n255\n", width, height).as_bytes());
        for chunk in pixels.chunks_exact(4) {
            buffer.push(chunk[0]);
            buffer.push(chunk[1]);
            buffer.push(chunk[2]);
        }
        if let Err(error) = std::fs::write(&path, buffer) {
            tracing::warn!(
                target: "brazen::render",
                path = %path.display(),
                %error,
                "failed to write snapshot"
            );
            self.shell_state
                .record_event(format!("snapshot save failed: {}", path.display()));
        } else {
            self.shell_state
                .record_event(format!("snapshot saved: {}", path.display()));
        }
    }

    pub(super) fn resolve_capture_dir(&self) -> std::path::PathBuf {
        let choice = self.config.engine.debug_capture_dir.trim();
        match choice {
            "" | "logs" => self.shell_state.runtime_paths.logs_dir.clone(),
            "data" => self.shell_state.runtime_paths.data_dir.clone(),
            "profiles" => self.shell_state.runtime_paths.profiles_dir.clone(),
            "cache" => self.shell_state.runtime_paths.cache_dir.clone(),
            "downloads" => self.shell_state.runtime_paths.downloads_dir.clone(),
            "crash_dumps" => self.shell_state.runtime_paths.crash_dumps_dir.clone(),
            value => std::path::PathBuf::from(value),
        }
    }

    pub(super) fn sample_pixel_rgba(pixels: &[u8], width: u32, height: u32) -> (u8, u8, u8, u8) {
        let width = width as usize;
        let height = height as usize;
        if width == 0 || height == 0 {
            return (0, 0, 0, 0);
        }
        let x = width / 2;
        let y = height / 2;
        let idx = (y.saturating_mul(width) + x).saturating_mul(4);
        if idx + 3 >= pixels.len() {
            return (0, 0, 0, 0);
        }
        (
            pixels[idx],
            pixels[idx + 1],
            pixels[idx + 2],
            pixels[idx + 3],
        )
    }
}
