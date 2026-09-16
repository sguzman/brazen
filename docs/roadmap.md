# Brazen Active Roadmap

Brazen now has **one active product roadmap**.

The older files under `docs/roadmaps/` remain useful as a capability inventory and historical design record, but they are not parallel active backlogs. Unchecked boxes in those files do not create obligations.

The governing rule is simple: **one active milestone, one bounded implementation task at a time.**

## Re-entry gate — verify reality before building

This is not a product milestone. It is the short evidence-gathering gate required after months away from the repository.

- [ ] Build the current tree on the actual Windows development machine.
- [ ] Run the existing test/check surface and record failures.
- [ ] Launch the desktop shell and capture current behavior.
- [ ] Verify current tab/window behavior; do not trust stale roadmap boxes.
- [ ] Verify whether ChatGPT can load, authenticate, render, accept input, and preserve session state in the current engine path.
- [ ] Smoke-test `codex app-server` locally and enumerate the thread/session/event primitives Brazen can consume.
- [ ] Verify whether an App Server client can attach to or correlate with threads already visible in the Codex desktop app.
- [ ] Update `docs/CURRENT_STATE.md` with observed results.

**Exit condition:** we have a factual compatibility matrix for Brazen browser behavior, ChatGPT, current tab behavior, and Codex App Server. No broad cleanup or Servo detour is required to exit this gate.

---

# Milestone 1 — ChatGPT <-> Codex Control Plane

Spec: [`docs/milestones/001-chatgpt-codex-control-plane.md`](./milestones/001-chatgpt-codex-control-plane.md)

This is the first product milestone.

## Slice 1 — Reliable ChatGPT workspace

- [ ] A real ChatGPT page is usable inside Brazen.
- [ ] Authentication/session persistence survives normal relaunches.
- [ ] Multiple ChatGPT conversations can be open as distinct tabs.
- [ ] Each tab has a stable Brazen-side identity independent of its visible title.
- [ ] Brazen can observe enough conversation activity to identify outbound user/assistant message events without relying on clipboard relay.

**Rule:** if the existing engine cannot run ChatGPT reliably, use the `BrowserEngine` seam to pursue the narrowest compatible engine path. General Servo completeness is not a milestone requirement.

## Slice 2 — Local Codex adapter

- [ ] Brazen can launch/connect to a Codex App Server boundary.
- [ ] Brazen can list/start/resume relevant Codex threads as supported by the current protocol.
- [ ] Brazen can send user/director input into a selected Codex thread.
- [ ] Brazen receives streaming Codex events and terminal/diff/approval state needed for the operator view.
- [ ] Adapter lifecycle and failures are visible in the UI.
- [ ] Relationship to the separately running Codex desktop app is documented from evidence, not assumption.

## Slice 3 — Explicit conversation bindings

- [ ] Operator can pair a ChatGPT conversation with a Codex thread.
- [ ] A pairing persists across ordinary restarts.
- [ ] Pairings can be disconnected and re-bound deliberately.
- [ ] No heuristic auto-pairing silently changes a binding.
- [ ] Each direction has independent policy state: **allow / hold / block**.

## Slice 4 — Live topology UI

- [ ] The shell visibly renders ChatGPT and Codex endpoints and the connections between them.
- [ ] Directionality is visible.
- [ ] Current policy state is visible directly on the connection.
- [ ] Connection health / failure state is visible.
- [ ] Active traffic is visually distinguishable from an idle connection.
- [ ] Multiple pairings can be understood without opening each conversation.

## Slice 5 — Traffic inspector and metrics

- [ ] Every routed event records source, destination, project/binding, timestamp, message/event type, delivery state, and payload size.
- [ ] Operator can inspect the full payload of an individual event.
- [ ] Operator can inspect bulk history for a connection.
- [ ] Per-direction metrics include at minimum messages/events per minute, bytes per minute, queued count/bytes, and failures.
- [ ] Traffic can be filtered by connection, direction, event type, delivery state, and time window.
- [ ] The causal chain of a failed handoff can be reconstructed from durable local state.

## Slice 6 — Allow / hold / block semantics

- [ ] **Allow:** eligible traffic forwards immediately.
- [ ] **Hold:** eligible traffic is retained in a visible queue and is not delivered until released or policy changes.
- [ ] **Block:** eligible traffic is not delivered; the rejected event/attempt remains auditable.
- [ ] Policy can be changed independently for ChatGPT -> Codex and Codex -> ChatGPT.
- [ ] Operator can release selected held events or a bounded batch.
- [ ] Queue growth and blocked traffic are obvious enough that the human cannot mistake silence for successful delivery.

## Milestone 1 exit criteria

Milestone 1 is complete when one Brazen window can demonstrate all of the following in ordinary use:

1. at least two usable ChatGPT tabs,
2. at least two selectable local Codex threads,
3. explicit persisted pairings,
4. real bidirectional routed traffic,
5. a live topology view,
6. per-direction allow/hold/block control,
7. per-connection traffic inspection and basic throughput/queue metrics,
8. recovery of who-sent-what-to-whom after an intentional held/blocked/failure scenario,
9. no human copy/paste required for the demonstrated ChatGPT <-> Codex handoff.

---

# After Milestone 1

Do not schedule these until Milestone 1 works.

## Milestone 2 — Supervised multi-project routing

Potential scope: project lanes, routing groups, batching, queue review, stronger provenance, project-specific policies, pause/park semantics, and richer operator alerts.

## Milestone 3 — Generalized agent / connector graph

Potential scope: additional agent runtimes, MCP tools, repository/file connectors, terminal surfaces, and reusable routing contracts.

## Milestone 4 — Re-evaluate the broader browser platform

Only after the control plane proves useful should Brazen reconsider older ambitions such as knowledge workflows, cache/asset tooling, reading/TTS, generalized virtual resources, and deeper browser replacement work.

The old roadmaps are evidence and inventory for that future decision, not commitments today.
