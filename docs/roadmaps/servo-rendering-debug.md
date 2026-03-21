# Servo Rendering Debug Roadmap

Focuses on eliminating the current psychedelic render output and establishing a correct, stable pixel pipeline.

## Pixel Format + Color Space

- [ ] Confirm Servo readback pixel format (RGBA/BGRA/ARGB) and document it
- [ ] Add a runtime pixel-format probe using a known solid-color test frame
- [ ] Validate sRGB vs linear conversion expectations for the readback buffer
- [ ] Verify alpha premultiplication and adjust if egui expects straight alpha
- [ ] Add an explicit pixel format enum to the upstream bridge for clarity

## Surface + Readback Integrity

- [ ] Validate render surface dimensions against the egui viewport every frame
- [ ] Confirm stride/row alignment from Servo readback and handle padding
- [ ] Ensure the readback rect uses correct origin and size (no off-by-one)
- [ ] Guard against zero-sized surfaces and skip readback cleanly
- [ ] Add a frame checksum to detect repeated or stale buffers

## Pipeline Wiring

- [ ] Verify WebRender pipeline ID and document lifecycle timing
- [ ] Confirm we are draining paint messages before readback
- [ ] Add a trace span around readback → upload with byte counts
- [ ] Add a debug toggle to bypass color conversion for A/B comparison
- [ ] Add a single-frame capture to disk (png) for offline inspection

## Input/Viewport Correlation

- [ ] Log viewport scale, device pixel ratio, and physical size each resize
- [ ] Verify scale factor usage matches Servo’s device pixel ratio expectations
- [ ] Validate pointer coordinates with a hit-test overlay
- [ ] Confirm scroll delta units match Servo’s expected units

## Validation + Tests

- [ ] Add a regression test that renders a known gradient and checks sample pixels
- [ ] Add a basic screenshot comparison test for about:blank
- [ ] Add a manual validation checklist to the README for visual sanity checks
