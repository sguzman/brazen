# ADR 0002 — Adopt Codex App Server as External Substrate

Status: **Accepted**  
Date: **2026-09-16**

## Context

Brazen's control-plane direction requires a machine-addressable Codex endpoint. Before re-baselining, that could have implied substantial custom work: agent runtime integration, thread persistence, event streaming, approvals, diffs, and potentially GUI automation of the Codex desktop application.

OpenAI now exposes the Codex harness through the official **Codex App Server** and describes App Server as the first-class integration method for clients going forward.

App Server already provides the Codex-native concerns a rich client needs, including:

- thread lifecycle and persistence,
- configuration and authentication,
- model discovery,
- tool execution and extensions,
- thread/turn/item lifecycle,
- streaming UI-ready events,
- diffs and execution state,
- server-initiated approval requests,
- a backward-compatible client protocol.

For local apps and IDEs, the documented pattern is to launch the Codex/App Server binary as a long-running child process and communicate bidirectionally using a JSON-RPC-lite protocol framed as JSONL over stdio. Protocol definitions can be generated from the Codex tooling.

## Decision

Brazen will treat Codex App Server as **external product substrate**, not as functionality to recreate.

Brazen's Codex integration will be a thin adapter that:

- owns process/connectivity lifecycle,
- negotiates/initializes the supported protocol,
- uses App Server thread/turn/item primitives,
- consumes native streaming events,
- forwards operator approval responses when needed,
- preserves native identifiers for provenance,
- translates only the minimum necessary state into Brazen control-plane concepts.

Brazen will not build a competing Codex harness.

## Scope removed from Brazen

The following are removed from active product scope unless App Server later proves insufficient in a concrete way:

- custom Codex agent loop,
- custom Codex thread persistence,
- custom Codex auth/config/model-discovery stack,
- custom tool/sandbox execution layer for Codex,
- custom Codex diff/event lifecycle,
- custom Codex approval protocol,
- rich general-purpose replacement for Codex Desktop,
- GUI automation of Codex Desktop as the normal integration architecture.

## What remains distinctly Brazen

App Server solves a Codex endpoint. It does not solve the cross-surface operator problem.

Brazen still owns:

- making ChatGPT Web routable,
- explicit ChatGPT conversation <-> Codex thread bindings,
- cross-surface routing,
- allow/hold/block direction policy,
- durable queues,
- provenance and audit history,
- project association,
- topology visualization,
- traffic/queue/failure metrics,
- operator inspection and intervention,
- causal reconstruction after failures.

## Event-model consequence

Brazen must distinguish **provider-native events** from **cross-surface routed traffic**.

One Codex turn can generate many App Server notifications. These should not automatically become messages on the Brazen wire or inflate routing metrics.

The adapter should classify native events into concepts such as:

- endpoint status/telemetry,
- operator action/approval,
- candidate routed content,
- provenance-only metadata.

Brazen's traffic metrics measure the wire, not every internal event emitted by Codex.

## Desktop app relationship

Whether Brazen can discover, resume, or correlate with threads simultaneously visible in a separately running Codex Desktop app remains an integration detail to verify.

Milestone 1 does not depend on direct attachment to the Desktop app. A Brazen-owned App Server process is acceptable.

## Consequences

### Positive

- removes a large amount of duplicate infrastructure from the roadmap,
- aligns Brazen with the supported Codex integration boundary,
- reduces fragility from desktop GUI automation,
- lets Brazen focus on its distinct coordination/control-plane value,
- gives Brazen structured thread IDs, events, approvals, and lifecycle information without inventing equivalents.

### Tradeoffs

- Brazen depends on App Server protocol availability and compatibility,
- App Server bugs/version changes become endpoint-adapter concerns,
- some formerly imagined 'Brazen infrastructure' is no longer owned by this project,
- the project becomes intentionally smaller.

That final consequence is accepted rather than treated as a defect.

## Reversal condition

Revisit this decision only if a concrete Brazen requirement cannot be met through App Server and cannot be solved by a thin adapter or supported protocol extension. Preference remains upstream/supportable integration over local reimplementation.
