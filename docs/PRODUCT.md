# Brazen Product Definition

## What Brazen is now

Brazen is a local operator control plane for coordinating browser conversations and local coding agents.

The immediate problem is not “build a complete new browser.” The immediate problem is that a human currently has to act as the message bus between ChatGPT and Codex: remembering which conversations correspond, moving payloads between surfaces, detecting failures, and reconstructing who sent what when something goes wrong.

Brazen should move that coordination burden into software.

## First product outcome

The first product milestone is a usable Brazen session in which the operator can:

1. open and use ChatGPT inside Brazen,
2. keep multiple ChatGPT conversations open as tabs,
3. connect Brazen to local Codex through a supported local adapter,
4. pair a ChatGPT conversation with a Codex thread,
5. see the pairings as a live visual topology,
6. inspect traffic in both directions,
7. see traffic volume, payload size, rate, queue state, and failures,
8. independently set ChatGPT -> Codex and Codex -> ChatGPT traffic to **allow**, **hold**, or **block**,
9. inspect individual transmitted payloads and bulk traffic history,
10. recover the causal chain after a failure without relying on human memory.

## Product principles

### The human is not the transport layer

Brazen exists to remove repetitive copying, routing, synchronization, and attribution work from the operator.

### Visible causality

Every routed event should make its origin, destination, policy decision, delivery state, and relationship to a conversation/thread inspectable.

### Human veto

Automation is subordinate to operator control. Each direction of each connection must be independently controllable.

### Durable state over chat memory

Bindings, policy state, queues, and traffic history should survive UI changes and should not depend on remembering what happened in a previous chat.

### Narrow vertical slices

The active milestone wins over platform completeness. Old capabilities remain available for reuse, but they do not compete for implementation attention.

### Local-first control plane

Routing, policy enforcement, and traffic inspection should run locally unless a remote dependency is intrinsic to the connected service.

## Explicit non-goals for the first milestone

The following are **not** first-milestone requirements:

- replacing the user’s general-purpose browser,
- completing Servo as a broadly compatible production browser engine,
- arbitrary filesystem/database/OCR/media connector support,
- knowledge graphs or ontology tooling,
- reading/TTS workflows,
- a generalized plugin marketplace,
- broad MCP orchestration unrelated to Codex,
- mobile support,
- cloud multi-user operation,
- autonomous agent-to-agent routing without visible operator controls.

These may return later if they serve the control-plane product.

## Existing platform assets worth preserving

Brazen already contains useful substrate for the new direction: an `egui`/`eframe` shell, browser-engine abstraction, session/profile persistence, permissions, automation APIs, event subscriptions, logging/audit concepts, and local capability seams. The project should reuse those pieces where they shorten the path to the first product outcome.
