# Brazen Product Definition

## What Brazen is now

Brazen is a local visual control plane for connections between AI work surfaces.

The first supported connection is:

**ChatGPT Web <-> Codex App Server**

Brazen does **not** aim to reimplement Codex, replace the Codex harness, or build a competing coding-agent runtime. OpenAI already exposes the Codex harness through the official Codex App Server, including thread lifecycle/persistence, config/auth, tool execution, streaming events, diffs, approvals, and other agent-loop state.

That changes Brazen's scope materially.

The immediate problem Brazen still owns is that a human currently has to act as the message bus between ChatGPT and Codex: remembering which conversations correspond, moving payloads between surfaces, deciding what is allowed to flow, detecting failures, and reconstructing who sent what when something goes wrong.

Brazen should move **routing, policy, provenance, topology, queueing, and cross-surface coordination** into software while delegating Codex-native behavior to App Server.

## Product boundary

### OpenAI / Codex App Server owns

- Codex agent loop,
- Codex thread lifecycle and persistence,
- Codex authentication and configuration,
- Codex model discovery,
- shell/file tool execution and sandbox policy,
- Codex-native skills/MCP participation,
- turn/item lifecycle,
- streaming agent events,
- diffs and command-execution state,
- approval requests and replies,
- Codex-native client protocol semantics.

Brazen should consume these capabilities through a thin adapter instead of recreating them.

### Brazen owns

- turning ChatGPT Web conversations into routable endpoints,
- explicit ChatGPT conversation <-> Codex thread bindings,
- cross-surface routing,
- per-direction **allow / hold / block** policy,
- visible queues,
- durable provenance and traffic history,
- cross-surface traffic metrics,
- project association,
- topology and health visualization,
- operator inspection and intervention,
- recovery of the causal chain after failures.

## First product outcome

The first product milestone is a usable Brazen session in which the operator can:

1. open and use ChatGPT inside Brazen,
2. keep multiple ChatGPT conversations open as tabs,
3. launch/connect to Codex through the official App Server boundary,
4. pair a ChatGPT conversation with an App Server thread,
5. see pairings as a live visual topology,
6. inspect traffic in both directions,
7. see traffic volume, payload size, rate, queue state, and failures,
8. independently set ChatGPT -> Codex and Codex -> ChatGPT traffic to **allow**, **hold**, or **block**,
9. inspect individual transmitted payloads and bulk traffic history,
10. recover the causal chain after a failure without relying on human memory.

## Product principles

### Do not rebuild solved substrate

If App Server exposes a Codex-native capability, Brazen consumes it. Brazen should not duplicate thread persistence, agent-loop orchestration, approval machinery, diff/event protocols, or generic Codex UI merely to prove independence.

### The human is not the transport layer

Brazen exists to remove repetitive copying, routing, synchronization, and attribution work from the operator.

### Visible causality

Every routed event should make its origin, destination, policy decision, delivery state, and relationship to a conversation/thread inspectable.

### Human veto

Automation is subordinate to operator control. Each direction of each connection must be independently controllable.

### Durable state over chat memory

Bindings, policy state, queues, and traffic history should survive UI changes and should not depend on remembering what happened in a previous chat.

### Thin endpoint adapters, thick coordination layer

Provider/runtime-specific semantics belong behind adapters. Brazen's durable product value lives above them: bindings, policy, routing, provenance, topology, metrics, and operator control.

### Local-first control plane

Routing, policy enforcement, and traffic inspection should run locally unless a remote dependency is intrinsic to the connected service.

## Explicit non-goals for the first milestone

The following are **not** first-milestone requirements:

- replacing the user’s general-purpose browser,
- completing Servo as a broadly compatible production browser engine,
- implementing a Codex-like agent runtime,
- implementing our own Codex thread store,
- implementing our own command/diff/approval protocol,
- reproducing the Codex Desktop UI,
- GUI automation of Codex Desktop when App Server provides the needed capability,
- arbitrary filesystem/database/OCR/media connector support,
- knowledge graphs or ontology tooling,
- reading/TTS workflows,
- a generalized plugin marketplace,
- broad MCP orchestration unrelated to cross-surface routing,
- mobile support,
- cloud multi-user operation,
- autonomous agent-to-agent routing without visible operator controls.

These may return later only if they serve the control-plane product and are not already better owned by an endpoint runtime.

## Existing platform assets worth preserving

Brazen already contains useful substrate for the new direction: an `egui`/`eframe` shell, browser-engine abstraction, session/profile persistence, permissions, automation APIs, event subscriptions, logging/audit concepts, and local capability seams. Reuse them where they shorten the path to the control plane.

The central architectural simplification is now explicit:

**Make ChatGPT routable. Adapt Codex App Server. Build the wire between them.**
