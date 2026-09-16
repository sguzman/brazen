# Milestone 001 — ChatGPT <-> Codex Control Plane

## Goal

Replace the human-as-message-bus workflow with a visible, inspectable, locally controlled connection between ChatGPT conversations and local Codex threads.

The milestone is successful when the operator can work on multiple projects without having to remember which ChatGPT conversation belongs to which Codex thread or manually relay ordinary messages between them.

## Operator story

The operator opens Brazen and sees several ChatGPT tabs. Beside or beneath them is a control-plane view containing the available Codex threads and explicit links between paired conversations.

A link answers, at a glance:

- what is connected,
- which direction traffic is allowed to flow,
- whether anything is queued or blocked,
- whether the connection is healthy,
- how much traffic is moving,
- and what the last meaningful event was.

The operator can open the connection inspector to see the exact payload history and causal chain.

## Architecture boundaries

### ChatGPT adapter

Responsibility:

- identify a Brazen tab hosting ChatGPT,
- identify the current ChatGPT conversation when possible,
- observe new relevant conversation events,
- submit routed input to the ChatGPT conversation,
- expose health/session state to the control plane.

Use existing Brazen browser/automation seams where practical. Keep ChatGPT-specific DOM/site assumptions isolated behind the adapter rather than scattering selectors and site logic through shell code.

### Codex adapter

First target: the official Codex App Server interface.

Responsibility:

- manage App Server process/connectivity,
- enumerate and identify Codex threads using the current protocol,
- send routed input,
- consume streaming events,
- surface approvals/failures/status needed by the operator,
- translate Codex protocol events into Brazen control-plane events.

Do not make the rest of Brazen depend directly on raw App Server JSON-RPC types.

Whether this adapter can attach to threads already owned by the separately running Codex desktop app is a **re-entry spike**, not a presumed fact.

### Control-plane core

The control-plane core owns bindings, policy, queues, routing decisions, event normalization, metrics, and durable history. UI code should render/control this state rather than becoming the routing implementation.

## Minimal domain model

Names may change during implementation, but the concepts should remain explicit.

### Endpoint

Represents one routable conversation surface.

Suggested fields:

- `endpoint_id`
- `kind` (`chatgpt` | `codex` initially)
- `external_id` when the connected system exposes one
- `display_label`
- `project_id` / project label when assigned
- `health`
- `last_seen_at`

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

A binding must never silently jump to a different endpoint because a title changed.

### Direction policy

Exactly three operator-facing states for Milestone 1:

- `allow`
- `hold`
- `block`

Semantics:

- **allow**: eligible event is forwarded immediately.
- **hold**: event is durably queued, visible, and releasable.
- **block**: event is not forwarded; the denied routing attempt remains auditable.

### Traffic event

Every routing attempt should become a normalized event with enough information to reconstruct the handoff.

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
- upstream/source event identifier when available
- downstream/delivery identifier when available
- error/failure detail when applicable

Useful delivery states include `observed`, `queued`, `forwarding`, `delivered`, `blocked`, `failed`, and `released`.

## UI requirements

### Topology view

The primary control-plane surface should visualize endpoints and bindings rather than hide them in a settings table.

For each binding show:

- ChatGPT endpoint,
- Codex endpoint,
- two directional lanes/arrows,
- policy state for each direction,
- activity/idle state,
- queued count/size,
- error state,
- compact traffic rate.

The operator should be able to change allow/hold/block directly from this surface.

### Traffic inspector

Selecting a connection opens a chronological event stream with:

- direction,
- timestamp,
- event type,
- payload size,
- policy decision,
- delivery state,
- payload preview/full inspection,
- failure information.

Support both individual event inspection and bulk/history views.

### Metrics

Minimum per-binding, per-direction metrics:

- events/messages per minute,
- bytes per minute,
- queued event count,
- queued bytes,
- blocked count,
- failed count,
- last successful delivery time.

Metrics are operational signals, not vanity dashboards. They should help answer “is this wire alive, jammed, blocked, or flooding?”

## Safety / control invariants

- No hidden auto-routing between unbound conversations.
- No silent policy changes.
- Held traffic must be visibly held.
- Blocked traffic must not be mistaken for delivered traffic.
- A failure in one binding must not silently reroute to another.
- Restarting Brazen must not erase the operator’s ability to explain the recent causal chain.
- UI titles are labels, not authoritative identities.
- Adapters may fail independently without corrupting binding/history state.

## Compatibility principle

The milestone optimizes for a reliable ChatGPT+Codex workflow, not browser-engine ideology.

If current Servo integration runs ChatGPT reliably, use it. If it does not, preserve the existing `BrowserEngine` abstraction and choose the narrowest engine path that satisfies the milestone. Do not turn “make all modern websites work in Servo” into a prerequisite for eliminating the human relay burden.

## Explicitly deferred

- generalized many-agent routing language,
- arbitrary graph cycles,
- autonomous agent-generated bindings,
- remote/cloud broker service,
- multi-user permissions,
- semantic message rewriting,
- generalized MCP marketplace,
- knowledge/TTS/cache workflows unrelated to this control plane.

## Demonstration scenario

The acceptance demo should deliberately include a failure/control case, not only the happy path:

1. Open two ChatGPT conversations in Brazen.
2. Connect each to a distinct Codex thread.
3. Allow traffic on the first pair and demonstrate a normal handoff.
4. Put ChatGPT -> Codex on the second pair into `hold`.
5. Produce traffic and show that it queues without delivery.
6. Inspect the queued payload and queue size.
7. Release it and show delivery.
8. Put Codex -> ChatGPT into `block` and generate a return event.
9. Show that the event is auditable as blocked and was not delivered.
10. Open the traffic history and reconstruct the full sequence without relying on clipboard history or memory.

If that scenario works, Brazen has begun doing the job it currently forces the human to do.
