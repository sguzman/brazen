# Observability And Quality Roadmap

Tracks tracing, diagnostics, testing, and verification quality across the platform.

## Current State

- [x] Tracing bootstrap exists
- [x] Console and rolling-file filters are configurable
- [x] Startup and command activity are logged
- [x] Config parsing tests exist
- [x] Path resolution tests exist
- [x] Command dispatch tests exist
- [x] Engine-state synchronization tests exist
- [x] Bootstrap integration tests exist

## Observability

- [x] Per-launch timestamped log files
- [ ] Engine lifecycle spans around real Servo integration
- [ ] Capability decision tracing
- [ ] Connector activity tracing
- [ ] Automation/API request tracing
- [ ] Cache/extraction pipeline tracing
- [ ] Diagnostics panel in the shell
- [ ] Metrics and health summaries
- [ ] CLI-based live log streaming and inspection

## Quality Gates

- [ ] GUI interaction smoke tests
- [ ] Servo-enabled integration checks on a prepared machine
- [ ] Cross-platform CI matrix
- [ ] Performance baselines for shell and engine startup
- [ ] Failure-injection tests for connector and automation paths
