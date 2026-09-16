# Brazen Active Roadmap

Brazen has **one active product roadmap**.

The older files under `docs/roadmaps/` remain capability inventory and historical design material. They are not parallel active backlogs.

The governing rule is: **one active milestone, one bounded implementation task at a time.**

A second governing rule now applies:

> **Do not rebuild Codex substrate that the official App Server already exposes.**

Brazen is not becoming a coding-agent runtime. It is becoming the cross-surface wire and operator control plane around existing endpoints.

## Re-entry gate — verify reality before building

This is an evidence-gathering gate, not a product milestone.

- [ ] Build the current tree on the actual Windows development machine.
- [ ] Run the existing test/check surface and record failures.
- [ ] Launch the desktop shell and capture current behavior.
- [ ] Verify current tab/window behavior; do not trust stale roadmap boxes.
- [ ] Verify whether ChatGPT can load, authenticate, render, accept input, and preserve session state in the current engine path.
- [ ] Run `codex app-server` locally.
- [ ] Generate/inspect the current App Server schema and record the exact thread/turn/item/event/approval surface available to Brazen.
- [ ] Confirm the preferred local-client lifecycle on Windows (child process / transport / shutdown / reconnect behavior).
- [ ] Verify whether a Brazen-owned App Server instance can discover, resume, or correlate with threads also visible in the Codex desktop app.
- [ ] Update `docs/CURRENT_STATE.md` with observed results.

**Exit condition:** we have a factual compatibility matrix for Brazen browser behavior, ChatGPT, current tab behavior, and current App Server behavior. No broad Servo work and no Codex-runtime reimplementation are required to exit this gate.

---

# Milestone 1 — Make ChatGPT routable and put a wire to Codex

Spec: [`docs/milestones/001-chatgpt-codex-control-plane.md`](./milestones/001-chatgpt-codex-control-plane.md)

Milestone 1 is intentionally asymmetrical:

- **ChatGPT Web is the hard endpoint integration.** Brazen must make it observable and writable enough to participate in routing.
- **Codex is already machine-addressable through App Server.** Brazen should implement only the thin client/adapter needed to consume that supported protocol.
- **Brazen's product value is the wire between them:** binding, routing, policy, queueing, provenance, topology, and inspection.

## Slice 1 — Reliable ChatGPT endpoint

- [ ] A real ChatGPT page is usable inside Brazen.
- [ ] Authentication/session persistence survives normal relaunches.
- [ ] Multiple ChatGPT conversations can be open as distinct tabs.
- [ ] Each tab/conversation has a stable Brazen-side identity independent of its visible title.
- [ ] Brazen can observe relevant new user/assistant message events without clipboard relay.
- [ ] Brazen can submit routed input to a selected ChatGPT conversation.
- [ ] ChatGPT-specific DOM/site logic is isolated behind one adapter boundary.

**Rule:** if the existing engine cannot run ChatGPT reliably, use the `BrowserEngine` seam to pursue the narrowest compatible engine path. General Servo completeness is not a milestone requirement.

## Slice 2 — Thin Codex App Server client

This slice is integration work, not invention.

- [ ] Brazen can launch/connect to the official Codex App Server using its supported local-client transport.
- [ ] Protocol bindings are generated or maintained from the official schema rather than handwritten ad hoc where practical.
- [ ] Brazen can enumerate/start/resume relevant Codex threads using App Server primitives.
- [ ] Brazen can submit input/turns to a selected thread.
- [ ] Brazen consumes streaming thread/turn/item notifications needed for operator status and traffic history.
- [ ] Brazen can surface and respond to server-initiated approval requests where relevant.
- [ ] Adapter lifecycle, version/protocol incompatibility, and failures are visible.
- [ ] Relationship to the separately running Codex desktop app is documented from evidence, not assumption.

**Explicitly out of scope:** implementing our own Codex agent loop, thread persistence, auth/config stack, tool runtime, diff protocol, approval protocol, or general Codex chat UI.

## Slice 3 — Explicit bindings

- [ ] Operator can pair a ChatGPT conversation with a Codex App Server thread.
- [ ] A pairing persists across ordinary Brazen restarts.
- [ ] Pairings can be disconnected and re-bound deliberately.
- [ ] No heuristic auto-pairing silently changes a binding.
- [ ] Each direction has independent policy state: **allow / hold / block**.

## Slice 4 — Routing and queue semantics

- [ ] **Allow:** eligible traffic forwards immediately.
- [ ] **Hold:** eligible traffic is retained in a visible durable queue and is not delivered until released or policy changes.
- [ ] **Block:** eligible traffic is not delivered; the rejected routing attempt remains auditable.
- [ ] Policy can be changed independently for ChatGPT -> Codex and Codex -> ChatGPT.
- [ ] Operator can release selected held events or a bounded batch.
- [ ] Adapter-native high-volume streaming events are classified so Brazen does not naively route every low-level Codex event back into ChatGPT.
- [ ] Routing rules distinguish operator-relevant messages from telemetry/status events.

## Slice 5 — Live topology UI

- [ ] The shell renders ChatGPT and Codex endpoints and their explicit bindings.
- [ ] Directionality is visible.
- [ ] Current allow/hold/block state is visible directly on each directional lane.
- [ ] Connection and adapter health are visible.
- [ ] Active traffic is visually distinguishable from idle state.
- [ ] Queue depth and basic throughput are visible without opening the inspector.
- [ ] Multiple project pairings can be understood at a glance.

## Slice 6 — Traffic inspector, metrics, and provenance

- [ ] Every cross-surface routing attempt records source, destination, binding/project, timestamp, normalized event type, policy decision, delivery state, and payload size.
- [ ] Operator can inspect the full routed payload of an individual event.
- [ ] Operator can inspect bulk history for a binding.
- [ ] App Server-native event identifiers/thread/turn/item IDs are retained as provenance where useful.
- [ ] Per-direction metrics include at minimum routed messages/events per minute, bytes per minute, queued count/bytes, blocked count, and failures.
- [ ] Internal App Server telemetry can be inspected without being confused with cross-surface routed traffic.
- [ ] The causal chain of a failed handoff can be reconstructed from durable local state.

## Milestone 1 exit criteria

Milestone 1 is complete when one Brazen window can demonstrate:

1. at least two usable ChatGPT conversations,
2. at least two selectable Codex App Server threads,
3. explicit persisted pairings,
4. real ChatGPT -> Codex routed traffic without human copy/paste,
5. real Codex -> ChatGPT routed return traffic without human copy/paste,
6. a live topology view,
7. per-direction allow/hold/block control,
8. per-connection traffic inspection and basic throughput/queue metrics,
9. recovery of who-sent-what-to-whom after an intentional held/blocked/failure scenario,
10. no duplicated Codex runtime machinery inside Brazen beyond the thin adapter and normalized control-plane state.

---

# After Milestone 1

Do not schedule these until Milestone 1 works.

## Milestone 2 — Supervised multi-project routing

Potential scope: project lanes, routing groups, batching, queue review, stronger provenance, project-specific policies, pause/park semantics, operator alerts, and richer project context association.

## Milestone 3 — Additional work-surface adapters

Only after the ChatGPT <-> Codex wire proves useful, consider other endpoints. Prefer native supported protocols where they exist. Brazen should remain a control plane, not absorb every endpoint's internal runtime.

Potential examples:

- other agent runtimes with supported APIs,
- selected MCP-backed work surfaces,
- repository/file/terminal endpoints where cross-surface routing creates clear value.

## Milestone 4 — Re-evaluate the old browser-platform ambitions

Only after the control plane proves useful should Brazen reconsider older ambitions such as knowledge workflows, cache/asset tooling, reading/TTS, generalized virtual resources, or deeper browser replacement work.

The old roadmaps are evidence and inventory for that future decision, not commitments today.
