# Brazen

Brazen is a Rust desktop project being narrowed into a **local visual control plane for connections between AI work surfaces**.

The first supported wire is:

**ChatGPT Web <-> Codex App Server**

Brazen is no longer trying to own the Codex runtime problem. OpenAI's official Codex App Server already exposes the Codex harness—threads, persistence, auth/config, tool execution, streaming events, diffs, approvals, and related agent-loop state—through a rich client protocol.

That discovery materially shrinks the project.

Brazen's job is now the layer App Server does **not** provide: making ChatGPT routable, binding work surfaces together, controlling what crosses the wire, preserving provenance, visualizing topology/health, and making failures explainable.

## Current Focus

**Milestone 1: ChatGPT Web <-> Codex App Server Control Plane**

Brazen should support:

- a reliable ChatGPT page inside Brazen,
- multiple ChatGPT conversations,
- a thin Codex App Server client/adapter,
- explicit ChatGPT-conversation <-> Codex-thread bindings,
- a live visual topology of those bindings,
- inspectable bidirectional routed traffic,
- messages/events per minute and bytes per minute on the wire,
- queue size and failure visibility,
- independent **allow / hold / block** policy for each direction,
- durable traffic history so failures can be reconstructed without relying on human memory.

Brazen should **not** reimplement:

- the Codex agent loop,
- Codex thread persistence,
- Codex auth/config/model discovery,
- Codex tool execution,
- Codex diff/approval/event semantics,
- a rich duplicate Codex Desktop UI,
- desktop GUI automation as the normal Codex integration path.

See:

- [`docs/PRODUCT.md`](docs/PRODUCT.md) — current product boundary and non-goals
- [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) — recovered project state and App Server scope change
- [`docs/roadmap.md`](docs/roadmap.md) — the single active product roadmap
- [`docs/milestones/001-chatgpt-codex-control-plane.md`](docs/milestones/001-chatgpt-codex-control-plane.md) — Milestone 1 specification
- [`docs/decisions/0001-control-plane-first.md`](docs/decisions/0001-control-plane-first.md) — why the control plane became the product
- [`docs/decisions/0002-adopt-codex-app-server.md`](docs/decisions/0002-adopt-codex-app-server.md) — why Codex runtime concerns are delegated to App Server
- [`AGENTS.md`](AGENTS.md) — human -> director -> grunt operating model
- [`.ai/TASK.md`](.ai/TASK.md) — next bounded implementation task

## Product Boundary

### Codex App Server gives Brazen a machine-addressable Codex endpoint

The intended integration is thin: launch/connect, initialize the protocol, list/start/resume threads, submit turns, consume streaming events, surface approvals, and preserve native IDs for provenance.

Provider-native Codex telemetry is **not automatically Brazen traffic**. One Codex turn may emit many internal events. Brazen must distinguish endpoint status/telemetry from actual content selected to cross the ChatGPT <-> Codex wire.

### Brazen owns the wire

The durable Brazen concepts are intentionally small:

- endpoint,
- binding,
- project association,
- direction policy,
- queue,
- routed traffic event,
- provenance,
- health/metrics.

The operator UI exists to make those concepts visible and controllable.

## Development Model

Brazen uses a **human -> director -> grunt** workflow:

- the human owns product intent and final decisions,
- ChatGPT acts as director: architecture, task decomposition, acceptance criteria, and review,
- Codex acts as implementer: bounded repository changes against an explicit task.

The repository is durable memory. Chat history is not the sole source of truth.

There is exactly **one active product milestone** at a time. The older files under `docs/roadmaps/` are retained as capability inventory and historical design material; they are not simultaneous active backlogs.

## Existing Brazen Foundation

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

## Architecture Rules For Milestone 1

### ChatGPT

The objective is reliable routability, not browser-engine purity.

If the current Servo path runs ChatGPT reliably, use it. If it does not, preserve the `BrowserEngine` boundary and pursue the narrowest compatible engine path rather than turning general modern-web compatibility into the first milestone.

### Codex

Use the official App Server boundary. Do not rebuild what it exposes.

Whether Brazen can discover/resume/correlate with threads simultaneously visible in a separately running Codex Desktop app is an evidence-gathering question, not a product dependency. A Brazen-owned App Server process is acceptable.

## Re-entry

The project was last implementation-heavy in late April 2026, with final `main` documentation activity on May 3, 2026. Before new feature work, run the evidence pass already staged in [`.ai/TASK.md`](.ai/TASK.md).

That pass will:

- build/test/launch the current tree,
- verify actual tab behavior,
- test ChatGPT load/input/session compatibility,
- launch and inspect the current Codex App Server/schema,
- classify App Server events for future routing design,
- test thread visibility/correlation with Codex Desktop without depending on it,
- classify old tracked logs/recovery artifacts,
- update `docs/CURRENT_STATE.md`,
- recommend the smallest first implementation task.

Do not begin by fixing unrelated Servo issues, reviving dormant platform tracks, or implementing Codex-native machinery.

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
- `src/automation/` — existing automation server/runtime/handlers
- `src/engine.rs` — browser engine abstraction
- `src/session.rs` — session/tab/window snapshot model
- `src/permissions.rs` — capability policy
- `src/profile_db.rs` — profile persistence
- `src/audit_log.rs` / `src/logging.rs` — audit and diagnostics
- `src/mcp.rs` / `src/mcp_stdio.rs` — existing MCP seams
- `src/virtual_protocol.rs` / `src/mounts.rs` — internal resource plumbing
- `src/servo_*` — optional Servo integration
- `docs/roadmaps/` — legacy capability inventory / historical planning
- `docs/decisions/` — architectural decisions
- `.ai/` — active agent task/report/blocker handoff state

## Historical Scope

Brazen previously treated engine embedding, permissions, cache access, local connectors, automation APIs, knowledge workflows, TTS/reading, virtual resources, and browser-shell UX as parallel roadmap tracks. It also implicitly carried more responsibility for the local coding-agent side because no narrow supported boundary had been incorporated into the project model.

Those ideas have not all been deleted, but they are dormant unless they serve the current product. Codex-native runtime functionality that App Server already owns is explicitly **not** part of Brazen's future backlog by default.

**Make ChatGPT routable. Adapt App Server. Build the wire.**
