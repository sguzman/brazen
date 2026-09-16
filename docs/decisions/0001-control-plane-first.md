# ADR 0001 — Control Plane First

Status: **Accepted for current product phase**  
Date: **2026-09-16**

## Context

Brazen accumulated a broad browser-platform scope: engine embedding, browser UX, permissions, cache, local connectors, automation, knowledge workflows, media/TTS, persistence, virtual resources, and observability. Separate roadmaps made each axis legible, but collectively created an execution model in which almost the whole platform appeared active at once.

The immediate user pain is much narrower and more concrete: ChatGPT and Codex are separate working surfaces, and the human is forced to carry messages, remember pairings, reconcile state, and diagnose failures across them.

## Decision

Brazen will prioritize a ChatGPT <-> Codex operator control plane before broader browser-platform work.

The first milestone will provide:

- reliable ChatGPT conversation tabs,
- a local Codex adapter,
- explicit persisted conversation/thread bindings,
- live visual topology,
- inspectable bidirectional traffic,
- per-direction allow/hold/block policy,
- queue/throughput/failure metrics,
- durable traffic history and attribution.

The old capability roadmaps remain in the repository as inventory and historical design material, but they do not constitute active parallel commitments.

## Consequences

### Positive

- Human coordination burden becomes the direct optimization target.
- Product progress can be demonstrated with a concrete end-to-end workflow.
- Existing automation, permissions, persistence, logging, and engine abstractions gain a focused use case.
- Browser-engine work is judged by whether it enables the milestone rather than by general completeness.

### Tradeoffs

- Some previously planned capabilities remain dormant.
- Servo-specific ambitions may be bypassed for the first milestone if they block reliable ChatGPT operation.
- Generalized connector/plugin architecture is deferred until the concrete ChatGPT/Codex wire works.

## Codex integration boundary

The official Codex App Server protocol is the preferred first adapter target because it is designed as a bidirectional local client interface.

Whether Brazen can attach to or correlate with threads already owned by a separately running Codex desktop app remains an empirical integration question. The architecture must keep that detail behind a Codex adapter boundary so the rest of the control plane does not depend on the answer.

## Reversal condition

Revisit this decision only after Milestone 1 works or evidence shows that the control-plane goal is technically infeasible without a different product shape.
