# Servo Real Rendering Implementation Roadmap

Focuses on replacing the current stub renderer with actual Servo output. This is the “make it render pages” plan.

## Servo Runtime Integration

- [x] Add Servo workspace path dependencies behind a `servo-upstream` feature
- [x] Introduce a `servo_upstream` module that wraps Servo types and embedder traits
- [x] Implement an `EventLoopWaker` that integrates with `eframe`’s repaint cadence
- [x] Build a minimal Servo `Embedder` implementation that can receive `EmbedderMsg`
- [x] Wire Servo logging to `tracing` with per-target filtering

## Rendering Pipeline

- [x] Initialize WebRender and compositor with the chosen backend
- [x] Create a render surface compatible with egui (CPU readback path)
- [x] Map Servo’s rendered frame into `egui::ColorImage`
- [x] Add a render loop that drains Servo paint messages each frame
- [x] Handle viewport resize and reallocate WebRender surfaces
- [x] Add explicit metrics for frame upload cost

## Navigation + Metadata

- [x] Instantiate a real Servo browser instance and WebView
- [x] Wire `navigate/reload/stop` to Servo’s API
- [x] Translate Servo navigation events into `EngineEvent`
- [x] Update title, URL, and favicon from Servo metadata
- [x] Surface load progress / document ready from Servo

## Input + Focus

- [x] Translate pointer events into Servo embedder input
- [x] Translate keyboard + modifiers into Servo input
- [x] Translate scroll/zoom events into Servo input
- [x] Wire IME composition to Servo
- [ ] Bridge clipboard read/write requests

## Validation

- [ ] Load `about:blank` without crashing and draw a frame
- [ ] Load a basic HTTP URL and show real content
- [ ] Add a smoke test that boots Servo and renders one real frame

## Tranche 1: Rendering Bootstrap (20 items)

- [x] Add `engine.startup_url` to the config schema for initial navigation
- [x] Validate `engine.startup_url` for supported schemes
- [x] Document `engine.startup_url` in the default TOML
- [x] Add a navigation helper module for URL normalization
- [x] Normalize address bar navigation before dispatching to the engine
- [x] Emit a navigation failure event for rejected inputs
- [x] Schedule startup navigation after the render surface is attached
- [x] Normalize new-window navigation targets before routing
- [x] Queue pending navigation in the Servo embedder when upstream is not ready
- [x] Flush pending navigation after upstream initialization
- [x] Track upstream active/error state in the Servo embedder
- [x] Log upstream init success with surface dimensions
- [x] Log the first upstream frame capture with format metadata
- [x] Surface upstream activity in `backend_name`
- [x] Refresh the shell backend name each sync cycle
- [x] Promote upstream errors into `EngineStatus::Error`
- [x] Sync active tab title and URL from upstream snapshots
- [x] Display render format metadata in the UI panel
- [x] Add URL normalization unit tests
- [x] Add config validation coverage for startup URLs
