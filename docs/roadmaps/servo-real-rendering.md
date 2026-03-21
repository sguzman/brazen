# Servo Real Rendering Roadmap

Turns the current scaffold into actual Servo-driven rendering with real page output.

## Servo Embedder Wiring

- [ ] Identify the correct Servo embedding crate/module entrypoints for the pinned revision
- [ ] Add Servo embedding crates to `Cargo.toml` (or path deps from `vendor/servo`)
- [ ] Create a dedicated `servo_runtime` module that isolates Servo-specific types
- [ ] Build a minimal Servo `Embedder` implementation compatible with the pinned revision
- [ ] Wire Servo logging to `tracing` (bridge Servo log targets)
- [ ] Add a runtime config surface for Servo feature flags (layout, webrender options)

## Window + Rendering Surface

- [ ] Create a real window/surface adapter that Servo can render into
- [ ] Initialize WebRender with the correct backend for the host platform
- [ ] Bridge the egui/eframe texture to the Servo surface (GPU or CPU path)
- [ ] Implement texture sharing or CPU readback with explicit performance notes
- [ ] Implement swapchain or frame scheduling for Servo’s compositor
- [ ] Make resize events reallocate compositor surfaces cleanly
- [ ] Add a frame pacing strategy tied to egui’s update cadence

## Navigation + Document Lifecycle

- [ ] Instantiate a real `Servo` browser instance and tab/session model
- [ ] Wire navigation requests into Servo’s browser API
- [ ] Surface navigation committed, loading, and idle events into `EngineEvent`
- [ ] Update title, URL, and favicon from Servo page metadata
- [ ] Translate Servo history into `NavigationState` back/forward
- [ ] Add page reload/stop hooks backed by Servo

## Input + Focus

- [ ] Map egui pointer events into Servo’s input pipeline
- [ ] Map keyboard events and modifiers accurately
- [ ] Map scroll and zoom events with correct delta units
- [ ] Wire IME composition events end-to-end
- [ ] Handle focus/blur and window activation transitions
- [ ] Add clipboard read/write support via Servo embedder hooks

## Diagnostics + Devtools

- [ ] Enable Servo devtools for the pinned revision
- [ ] Expose devtools endpoint in shell (already surfaced) and validate connectivity
- [ ] Add a “render mode” indicator: CPU readback vs GPU texture
- [ ] Record per-frame timings and surface in the log panel
- [ ] Capture Servo crashes and emit structured crash dumps

## Build + Tooling

- [ ] Document the exact Servo build steps for Linux/macOS/Windows
- [ ] Add a build helper (justfile/xtask) to build Servo then Brazen
- [ ] Add a CI job that builds Servo artifacts and then Brazen with `--features servo`
- [ ] Add a smoke-test that boots a Servo tab and renders a single frame
