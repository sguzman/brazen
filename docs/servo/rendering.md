# Servo Rendering Modes

Brazen currently supports a CPU readback path for frames rendered by the Servo embedder
scaffold. This keeps the interface compatible with `egui` textures while we wire the real
GPU compositor.

## Modes

- `cpu-readback`: the embedder produces an RGBA buffer that is uploaded to an `egui` texture.
  - Pros: simple, works everywhere.
  - Cons: copies every frame, CPU heavy, not suitable for high frame rates.
- `gpu-texture`: placeholder for a future GPU texture bridge (not wired yet).

## Performance Notes

CPU readback requires allocating and uploading a full RGBA buffer for every frame. On a
1440x920 surface this is ~5MB per frame. Expect high CPU usage while we remain in this mode.

Frame pacing is controlled by `engine.frame_pacing`:

- `vsync`: repaint every frame.
- `manual`: throttled to ~60fps.
- `on-demand`: render only after input/navigation events.
