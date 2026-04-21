# Servo Live Integration Roadmap

Focuses on turning the current Servo scaffold into an actual embedded renderer that loads and displays pages.

## Build And Sources

- [ ] Pin a specific Servo source revision with a reproducible fetch script
- [ ] Add Servo as a git dependency or workspace submodule
- [ ] Document platform-specific build prerequisites for the pinned revision
- [ ] Establish a dedicated build profile for Servo artifacts
- [ ] Add CI target that builds Servo in the same environment

## Embedder Runtime

- [ ] Create a Servo embedder crate/module wired to the pinned source
- [ ] Implement real init/shutdown and error propagation
- [ ] Initialize Servo’s renderer and compositor
- [ ] Allocate a render surface compatible with `egui` textures
- [ ] Upload rendered frames to the `egui` surface each frame
- [ ] Implement window resize handling with framebuffer reallocation

## Input And Event Loop

- [ ] Forward mouse/pointer events to Servo correctly
- [ ] Forward keyboard events to Servo correctly
- [ ] Forward scroll/zoom events to Servo correctly
- [ ] Wire IME composition into Servo text input
- [ ] Add focus/blur events for window activation
- [ ] Integrate Servo’s event loop with `eframe`’s update cadence

## Navigation And State

- [ ] Translate Servo navigation events into shell events
- [ ] Update title/favicon from Servo
- [ ] Wire back/forward stack to Servo history
- [ ] Implement reload/stop commands
- [ ] Surface load progress updates

## Diagnostics And Debugging

- [ ] Add a render debug overlay in the shell
- [ ] Capture Servo logs and pipe into `tracing`
- [ ] Add a runtime toggle for verbose Servo logging
- [ ] Implement a minimal devtools transport for local use

## Stability

- [ ] Crash detection with retry/backoff
- [ ] Persist crash dumps with session context
- [ ] Memory and GPU resource cleanup on shutdown
