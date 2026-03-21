# Servo Build Steps

These steps reflect the Servo README for the pinned revision. Use them to build Servo in
`vendor/servo` before enabling Brazen’s `servo` feature.

## Linux

```bash
cd vendor/servo
./mach bootstrap
./mach build
```

## macOS

```bash
cd vendor/servo
./mach bootstrap
./mach build
```

## Windows (PowerShell)

```powershell
cd vendor/servo
.\mach bootstrap
.\mach build
```

## Notes

- Servo uses its own build system. Brazen does not compile Servo automatically.
- After Servo builds, set `BRAZEN_SERVO_SOURCE=vendor/servo` and build Brazen with
  `cargo build --features servo`.
- Brazen patches `glslopt` via `vendor/glslopt` to avoid a `once_flag/call_once`
  conflict on newer glibc.
