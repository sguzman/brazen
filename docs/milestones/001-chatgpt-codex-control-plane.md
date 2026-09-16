# Milestone 001 — ChatGPT <-> Codex Control Plane

## Goal

Replace the human-as-message-bus workflow with a visible, inspectable, locally controlled wire between **ChatGPT Web** and **Codex App Server**.

This milestone does **not** build Codex. It consumes Codex through OpenAI's first-class App Server interface and concentrates Brazen's original effort on the unsolved cross-surface problem: making ChatGPT routable, pairing endpoints, controlling traffic, preserving provenance, and visualizing the wire.

## The architectural simplification

OpenAI already owns the Codex harness:

- agent loop,
- thread lifecycle and persistence,
- config/auth,
- model discovery,
- tool execution,
- turn/item event stream,
- diffs,
- approvals,
- Codex-native protocol semantics.

Brazen must not recreate those systems.

Milestone 1 therefore has three real pieces:

1. **ChatGPT adapter** — turn the website into a reliable routable endpoint.
2. **Codex App Server adapter** — a thin supported client for an already-machine-addressable endpoint.
3. **Brazen wire/control plane** — bindings, routing, allow/hold/block, queues, provenance, metrics, topology, and operator intervention.

## Operator story

The operator opens Brazen and sees several ChatGPT conversations and several Codex App Server threads. Explicit links show which conversation is paired with which thread.

A link answers at a glance:

- what is connected,
- which direction traffic is allowed to flow,
- whether anything is queued or blocked,
- whether each endpoint is healthy,
- how much cross-surface traffic is moving,
- and what the last meaningful routed event was.

The operator can open the connection inspector to see the exact routed payload history and causal chain while still being able to inspect richer Codex-native status/events separately.

## Architecture boundaries

### ChatGPT adapter

Responsibility:

- identify a Brazen tab hosting ChatGPT,
- identify the current ChatGPT conversation when possible,
- observe new relevant conversation events,
- submit routed input to the selected conversation,
- expose health/session state to the control plane.

ChatGPT-specific DOM/site assumptions must remain isolated behind the adapter rather than leaking into routing/UI code.

### Codex App Server adapter

This is intentionally thin.

Responsibility:

- manage App Server process/connectivity,
- initialize the supported protocol,
- enumerate/start/resume Codex threads,
- submit turns/input,
- consume streaming thread/turn/item events,
- surface server-initiated approvals to the operator when needed,
- translate only the necessary Codex-native events into Brazen endpoint/status/provenance concepts,
- expose version/health/failure state.

The adapter must **not** become a second Codex harness.

Do not implement custom replacements for App Server thread persistence, tool execution, auth/config, model discovery, diff semantics, approval semantics, or event lifecycle.

Whether Brazen can directly correlate with threads simultaneously visible in the separately running Codex desktop app remains an empirical integration question, not a milestone dependency.

### Control-plane core

The control-plane core owns the cross-surface concepts App Server intentionally does not own:

- endpoint bindings,
- project association,
- direction policy,
- queues,
- routing decisions,
- normalized routed events,
- provenance,
- cross-surface metrics,
- durable traffic history.

UI code renders and controls this state; it does not become the router.

## Minimal domain model

### Endpoint

Represents one routable work surface.

Suggested fields:

- `endpoint_id`
- `kind` (`chatgpt` | `codex` initially)
- `external_id`
- `display_label`
- `project_id`
- `health`
- `last_seen_at`

For Codex, preserve App Server thread identity instead of inventing a competing conversation identity.

### Binding

Represents an explicit relationship between two endpoints.

Suggested fields:

- `binding_id`
- `chatgpt_endpoint_id`
- `codex_endpoint_id`
- `chatgpt_to_codex_policy`
- `codex_to_chatgpt_policy`
- `created_at`
- `updated_at`

Bindings must never silently jump because a visible title changed.

### Direction policy

Exactly three operator-facing states for Milestone 1:

- `allow`
- `hold`
- `block`

Semantics:

- **allow** — eligible routed traffic forwards immediately.
- **hold** — eligible routed traffic is durably queued, visible, and releasable.
- **block** — eligible routed traffic is not delivered; the denied routing attempt remains auditable.

### Routed traffic event

A routed event is **not the same thing as every raw App Server event**.

App Server may produce many low-level/status events for one turn. Brazen should retain useful native provenance without blindly forwarding or counting every internal delta as a cross-surface message.

Minimum fields:

- `event_id`
- `binding_id`
- `source_endpoint_id`
- `destination_endpoint_id`
- `direction`
- `event_type`
- `created_at`
- `policy_at_decision`
- `delivery_state`
- `payload_size_bytes`
- `payload` or durable payload reference
- upstream/source identifier when available
- downstream/delivery identifier when available
- Codex thread/turn/item IDs when relevant
- error/failure detail when applicable

Useful delivery states include `observed`, `queued`, `forwarding`, `delivered`, `blocked`, `failed`, and `released`.

## UI requirements

### Topology view

The primary control-plane surface visualizes endpoints and bindings rather than reproducing the Codex Desktop application.

For each binding show:

- ChatGPT endpoint,
- Codex thread endpoint,
- two directional lanes/arrows,
- policy state for each direction,
- activity/idle state,
- queued count/size,
- endpoint/adapter health,
- compact routed traffic rate.

The operator can change allow/hold/block directly from this surface.

### Traffic inspector

Selecting a binding opens a chronological routed-event stream with:

- direction,
- timestamp,
- normalized event type,
- payload size,
- policy decision,
- delivery state,
- payload preview/full inspection,
- failure information,
- relevant native provenance IDs.

Codex-native telemetry/status may have a secondary inspectable view, but it must not be confused with the cross-surface wire.

### Metrics

Minimum per-binding, per-direction metrics:

- routed messages/events per minute,
- bytes per minute,
- queued event count,
- queued bytes,
- blocked count,
- failed count,
- last successful delivery time.

These answer: **is this wire alive, jammed, blocked, or flooding?**

## Safety / control invariants

- No hidden auto-routing between unbound conversations.
- No silent policy changes.
- Held traffic must be visibly held.
- Blocked traffic must not be mistaken for delivered traffic.
- A failure in one binding must not silently reroute to another.
- Restarting Brazen must not erase the operator's ability to explain the recent causal chain.
- UI titles are labels, not authoritative identities.
- App Server failures must not corrupt Brazen binding/history state.
- Brazen must distinguish provider-native telemetry from actual cross-surface traffic.

## Compatibility principle

The milestone optimizes for a reliable ChatGPT + App Server workflow, not browser-engine ideology and not Codex-runtime independence.

If current Servo integration runs ChatGPT reliably, use it. If not, preserve the `BrowserEngine` abstraction and choose the narrowest engine path that satisfies the milestone.

For Codex, prefer the supported App Server protocol even when implementing the same behavior ourselves would be technically possible.

## Explicitly deferred / removed from active scope

- our own Codex agent loop,
- our own Codex conversation persistence,
- our own Codex approval/diff/tool protocol,
- rich duplicate Codex chat UI,
- Codex Desktop GUI automation,
- generalized many-agent routing language,
- arbitrary graph cycles,
- autonomous agent-generated bindings,
- remote/cloud broker service,
- multi-user permissions,
- semantic message rewriting,
- generalized MCP marketplace,
- knowledge/TTS/cache workflows unrelated to this control plane.

## Demonstration scenario

1. Open two ChatGPT conversations in Brazen.
2. Discover/start/resume two Codex App Server threads.
3. Explicitly pair each ChatGPT conversation to one Codex thread.
4. Allow ChatGPT -> Codex on the first pair and demonstrate a normal handoff.
5. Show Codex turn/status activity from App Server without pretending all native events are routed messages.
6. Put ChatGPT -> Codex on the second pair into `hold`.
7. Produce traffic and show that it queues without delivery.
8. Inspect the queued payload and queue size.
9. Release it and show delivery into the selected App Server thread.
10. Put Codex -> ChatGPT into `block` and generate an eligible return/report event.
11. Show that the event is auditable as blocked and was not delivered.
12. Open traffic history and reconstruct the full sequence without clipboard history or memory.

If that scenario works, Brazen is doing the part App Server does not do: **controlling and explaining the wire between work surfaces.**
