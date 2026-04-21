# Servo Real Rendering Implementation Roadmap

Focuses on replacing the current stub renderer with actual Servo output. This is the “make it render pages” plan.

## Servo Runtime Integration

- [ ] Add Servo workspace path dependencies behind a `servo-upstream` feature
- [ ] Introduce a `servo_upstream` module that wraps Servo types and embedder traits
- [ ] Implement an `EventLoopWaker` that integrates with `eframe`’s repaint cadence
- [ ] Build a minimal Servo `Embedder` implementation that can receive `EmbedderMsg`
- [ ] Wire Servo logging to `tracing` with per-target filtering

## Rendering Pipeline

- [ ] Initialize WebRender and compositor with the chosen backend
- [ ] Create a render surface compatible with egui (CPU readback path)
- [ ] Map Servo’s rendered frame into `egui::ColorImage`
- [ ] Add a render loop that drains Servo paint messages each frame
- [ ] Handle viewport resize and reallocate WebRender surfaces
- [ ] Add explicit metrics for frame upload cost

## Navigation + Metadata

- [ ] Instantiate a real Servo browser instance and WebView
- [ ] Wire `navigate/reload/stop` to Servo’s API
- [ ] Translate Servo navigation events into `EngineEvent`
- [ ] Update title, URL, and favicon from Servo metadata
- [ ] Surface load progress / document ready from Servo

## Input + Focus

- [ ] Translate pointer events into Servo embedder input
- [ ] Translate keyboard + modifiers into Servo input
- [ ] Translate scroll/zoom events into Servo input
- [ ] Wire IME composition to Servo
- [ ] Bridge clipboard read/write requests

## Validation

- [ ] Load `about:blank` without crashing and draw a frame
- [ ] Load a basic HTTP URL and show real content
- [ ] Add a smoke test that boots Servo and renders one real frame

## Tranche 1: Rendering Bootstrap (20 items)

- [ ] Add `engine.startup_url` to the config schema for initial navigation
- [ ] Validate `engine.startup_url` for supported schemes
- [ ] Document `engine.startup_url` in the default TOML
- [ ] Add a navigation helper module for URL normalization
- [ ] Normalize address bar navigation before dispatching to the engine
- [ ] Emit a navigation failure event for rejected inputs
- [ ] Schedule startup navigation after the render surface is attached
- [ ] Normalize new-window navigation targets before routing
- [ ] Queue pending navigation in the Servo embedder when upstream is not ready
- [ ] Flush pending navigation after upstream initialization
- [ ] Track upstream active/error state in the Servo embedder
- [ ] Log upstream init success with surface dimensions
- [ ] Log the first upstream frame capture with format metadata
- [ ] Surface upstream activity in `backend_name`
- [ ] Refresh the shell backend name each sync cycle
- [ ] Promote upstream errors into `EngineStatus::Error`
- [ ] Sync active tab title and URL from upstream snapshots
- [ ] Display render format metadata in the UI panel
- [ ] Add URL normalization unit tests
- [ ] Add config validation coverage for startup URLs

## Tranche 2: Resource Reader + Deep Diagnostics (20 items)

- [ ] Add `engine.servo_resources_dir` to the config schema
- [ ] Resolve the resources directory from config, env var, or vendor path
- [ ] Implement a Servo resource reader for required files
- [ ] Initialize the resource reader before Servo builder creation
- [ ] Emit a clear error when the resources directory cannot be resolved
- [ ] Track resource reader readiness in the Servo embedder
- [ ] Surface resource reader status in render health updates
- [ ] Add a render frame probe for non-white ratio and alpha stats
- [ ] Log a warning after 30 mostly-white frames post navigation
- [ ] Display probe stats in the backend UI panel
- [ ] Display render health (reader/upstream/load status/error) in the UI
- [ ] Track load status transitions from Servo snapshots
- [ ] Emit warnings when load status is stuck at Started for 10s
- [ ] Reset blank-frame counters on new navigation
- [ ] Add unit tests for resource resolution order
- [ ] Add unit test for resource reader file loading
- [ ] Add config wiring for debug frame probe in dev mode
- [ ] Keep debug pixel probe intact for format detection
- [ ] Gate probe metrics behind config toggle
- [ ] Preserve per-launch log behavior for diagnostics

## Tranche 3: White Render Fixes (4 items)

- [ ] Read back pixels before presenting the software render surface
- [ ] Propagate viewport resize to Servo WebView instances
- [ ] Log render capture success/failure with probe alpha and sample RGB values
- [ ] Add a data-URL smoke test that asserts non-white pixels render

## Tranche 4: TLS + Connectivity Defaults (3 items)

- [ ] Add config for `engine.ignore_certificate_errors` and optional `engine.certificate_path`
- [ ] Wire TLS options into Servo `Opts` during upstream initialization
- [ ] Default ignore-certificate-errors to dev mode when not explicitly set

## Tranche 5: System CA Auto-Detection (1 item)

- [ ] Auto-detect a system CA bundle path when none is configured
