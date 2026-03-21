# Servo Embedding Prerequisites

This document tracks the minimum host requirements for Servo integration.

## Environment

- `BRAZEN_SERVO_SOURCE` must point to a local Servo checkout when building with `--features servo`.
- A Rust toolchain compatible with the Servo checkout is required.
- A C/C++ toolchain compatible with Servo dependencies is required.

## Linux (baseline)

- OpenGL or Vulkan graphics stack available (depending on `engine.gfx_backend`).
- X11 and/or Wayland development headers present.
- System dependencies required by Servo’s build scripts.

## Windows

- Visual Studio Build Tools compatible with Servo.
- Graphics runtime matching the selected backend.

## macOS

- Xcode CLI tools.
- Metal/OpenGL runtime matching the selected backend.

## Current Default

- Process model: single-process.
- Graphics backend: `gl`.
