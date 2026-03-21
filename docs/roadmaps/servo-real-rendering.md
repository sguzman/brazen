# Servo Real Rendering Roadmap

Turns the current scaffold into actual Servo-driven rendering with real page output.

## Servo Embedder Wiring

- [x] Identify the correct Servo embedding crate/module entrypoints for the pinned revision
- [x] Add Servo embedding crates to `Cargo.toml` (or path deps from `vendor/servo`)
- [x] Create a dedicated `servo_runtime` module that isolates Servo-specific types
- [x] Build a minimal Servo `Embedder` implementation compatible with the pinned revision
- [x] Wire Servo logging to `tracing` (bridge Servo log targets)
- [x] Add a runtime config surface for Servo feature flags (layout, webrender options)

## Window + Rendering Surface

- [x] Create a real window/surface adapter that Servo can render into
- [x] Initialize WebRender with the correct backend for the host platform
- [x] Bridge the egui/eframe texture to the Servo surface (GPU or CPU path)
- [x] Implement texture sharing or CPU readback with explicit performance notes
- [x] Implement swapchain or frame scheduling for Servo’s compositor
- [x] Make resize events reallocate compositor surfaces cleanly
- [x] Add a frame pacing strategy tied to egui’s update cadence

## Navigation + Document Lifecycle

- [x] Instantiate a real `Servo` browser instance and tab/session model
- [x] Wire navigation requests into Servo’s browser API
- [x] Surface navigation committed, loading, and idle events into `EngineEvent`
- [x] Update title, URL, and favicon from Servo page metadata
- [x] Translate Servo history into `NavigationState` back/forward
- [x] Add page reload/stop hooks backed by Servo

## Input + Focus

- [x] Map egui pointer events into Servo’s input pipeline
- [x] Map keyboard events and modifiers accurately
- [x] Map scroll and zoom events with correct delta units
- [x] Wire IME composition events end-to-end
- [x] Handle focus/blur and window activation transitions
- [x] Add clipboard read/write support via Servo embedder hooks

## Diagnostics + Devtools

- [x] Enable Servo devtools for the pinned revision
- [x] Expose devtools endpoint in shell (already surfaced) and validate connectivity
- [x] Add a “render mode” indicator: CPU readback vs GPU texture
- [x] Record per-frame timings and surface in the log panel
- [x] Capture Servo crashes and emit structured crash dumps

## Build + Tooling

- [x] Document the exact Servo build steps for Linux/macOS/Windows
- [x] Add a build helper (justfile/xtask) to build Servo then Brazen
- [x] Add a CI job that builds Servo artifacts and then Brazen with `--features servo`
- [x] Add a smoke-test that boots a Servo tab and renders a single frame
