# Servo Embedding Entrypoints

This document captures the primary Servo embedding entrypoints for the pinned revision
(`vendor/servo` at `b73ae02`). The goal is to keep Brazen’s integration aligned with Servo’s
current public surfaces.

## Core Engine Handle

- `vendor/servo/components/servo/servo.rs`
  - `ServoBuilder` (constructs a `Servo` instance)
  - `Servo` (the in-process engine handle)
  - `EventLoopWaker` (embedder callback for pumping the event loop)

## Embedder Traits

- `vendor/servo/components/shared/embedder`
  - `EmbedderMsg` / `EmbedderToConstellationMessage` (messages sent from Servo to embedder)
  - Input, clipboard, and dialog request traits used by the embedders

## Reference Implementation

- `vendor/servo/ports/servoshell`
  - `running_app_state.rs` and `window.rs` show how Servo wires input, navigation, and rendering.
  - `desktop/gui.rs` shows how images are mapped into `egui` for servoshell’s UI.

## Notes

Brazen’s `servo_embedder.rs` mirrors the shape of `servoshell` but keeps the implementation
as a scaffold until we wire the real Servo compositor and event loop.

When you are ready to wire real Servo types into Brazen, add path dependencies to the
Servo workspace (requires `vendor/servo` to be checked out):

```toml
[dependencies]
libservo = { path = "vendor/servo/components/servo", package = "libservo" }
embedder_traits = { path = "vendor/servo/components/shared/embedder" }
```
