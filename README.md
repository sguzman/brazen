# Brazen

Brazen is a Rust desktop browser/runtime project being narrowed into a **local operator control plane for browser conversations and local coding agents**.

The repository already contains a substantial browser/platform foundation: an `egui` / `eframe` shell, browser-engine abstraction, session/profile persistence, permissions, automation APIs, cache/extraction tooling, MCP seams, and optional Servo integration. The current product goal is **not** to finish all of those surfaces at once.

The immediate goal is to remove the human from the role of manual message bus between ChatGPT and Codex.

## Current Focus

**Milestone 1: ChatGPT <-> Codex Control Plane**

Brazen should support:

- a reliable ChatGPT page inside Brazen,
- multiple ChatGPT conversation tabs,
- a local Codex adapter,
- explicit ChatGPT-conversation <-> Codex-thread bindings,
- a live visual topology of those bindings,
- inspectable bidirectional traffic,
- messages/events per minute and bytes per minute,
- queue size and failure visibility,
- independent **allow / hold / block** policy for each direction,
- durable traffic history so failures can be reconstructed without relying on human memory.

See:

- [`docs/PRODUCT.md`](docs/PRODUCT.md) — current product definition and non-goals
- [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) — recovered project state and re-entry notes
- [`docs/roadmap.md`](docs/roadmap.md) — the single active product roadmap
- [`docs/milestones/001-chatgpt-codex-control-plane.md`](docs/milestones/001-chatgpt-codex-control-plane.md) — Milestone 1 specification
- [`AGENTS.md`](AGENTS.md) — human -> director -> grunt operating model
- [`.ai/TASK.md`](.ai/TASK.md) — next bounded implementation task

## Development Model

Brazen uses a **human -> director -> grunt** workflow:

- the human owns product intent and final decisions,
- ChatGPT acts as director: architecture, task decomposition, acceptance criteria, and review,
- Codex acts as implementer: bounded repository changes against an explicit task.

The repository is durable memory. Chat history is not the sole source of truth.

There is exactly **one active product milestone** at a time. The older files under `docs/roadmaps/` are retained as a capability inventory and historical design record; they are not simultaneous active backlogs.

## Existing Foundation

The current repository already includes:

- native `eframe` / `egui` desktop shell,
- `BrowserEngine` abstraction,
- session/tab/window/navigation/recovery concepts,
- typed configuration and runtime path handling,
- profile persistence,
- capability-based permissions,
- logging and audit infrastructure,
- cache/asset storage,
- HTML/entity extraction,
- WebSocket automation server,
- CLI introspection/control client,
- DOM/tab/window automation surfaces,
- event subscriptions and rate/backpressure concepts,
- MCP and virtual-resource seams,
- optional Servo-backed engine integration.

These are reusable substrate, not requirements to expand before Milestone 1.

## Architecture Rule For Milestone 1

The objective is a reliable ChatGPT + Codex workflow, not browser-engine purity.

If the current Servo path runs ChatGPT reliably, use it. If it does not, preserve the `BrowserEngine` boundary and pursue the narrowest compatible engine path rather than turning general modern-web compatibility into the first milestone.

For Codex, the preferred integration boundary is the official Codex App Server protocol. Whether Brazen can directly attach to or correlate with threads already owned by a separately running Codex desktop app is intentionally treated as an evidence-gathering question, not an assumption.

## Re-entry

The project was last implementation-heavy in late April 2026, with final `main` documentation activity on May 3, 2026. Before new feature work, run the evidence pass already staged in [`.ai/TASK.md`](.ai/TASK.md).

That pass is expected to:

- build/test/launch the current tree,
- verify actual tab behavior,
- test ChatGPT load/input/session compatibility,
- smoke-test Codex App Server,
- determine what relationship is possible with the Codex desktop app,
- classify old tracked logs/recovery artifacts,
- update `docs/CURRENT_STATE.md`,
- recommend the smallest first implementation task.

Do not begin by fixing unrelated Servo issues or reviving dormant platform tracks.

## Building

Standard commands:

```bash
cargo build
cargo test
cargo run
```

Servo-backed paths are optional features and may require the vendored Servo source tree and native prerequisites.

```bash
cargo build --features servo
```

The checked-in `config/brazen.toml` documents the configuration shape.

## Repository Map

- `src/app/` — desktop shell and operator UI
- `src/automation/` — automation server/runtime/handlers
- `src/engine.rs` — browser engine abstraction
- `src/session.rs` — session/tab/window snapshot model
- `src/permissions.rs` — capability policy
- `src/profile_db.rs` — profile persistence
- `src/audit_log.rs` / `src/logging.rs` — audit and diagnostics
- `src/mcp.rs` / `src/mcp_stdio.rs` — MCP seams
- `src/virtual_protocol.rs` / `src/mounts.rs` — internal resource plumbing
- `src/servo_*` — optional Servo integration
- `docs/roadmaps/` — legacy capability inventory / historical planning
- `.ai/` — active agent task/report/blocker handoff state

## Historical Scope

Brazen previously treated engine embedding, permissions, cache access, local connectors, automation APIs, knowledge workflows, TTS/reading, virtual resources, and browser-shell UX as parallel roadmap tracks.

Those ideas have not been deleted. They are simply dormant until they serve the current control-plane product.

**Build the wire first.**
