# Brazen Current State

Recovered and re-baselined: **2026-09-16**.

This document is the first place to look after a long absence.

## Repository identity

- GitHub repository: `sguzman/brazen`
- Default branch: `main`
- Primary language: Rust
- Desktop shell: `eframe` / `egui`
- The project has historically been called both “Brazen” and “Braizen” conversationally. Do not spend re-entry time on a rename.

## Where work actually stopped

The last implementation-heavy stretch was in late April 2026. The final feature commits concentrated on browser usability and Servo integration: input handling, clipboard/copy-paste, scroll behavior, render/input ordering, panel sizing, shortcuts, and shell modularization.

The last repository activity on `main` was documentation cleanup on **2026-05-03**. There are no open/recent project PRs carrying a known unfinished implementation branch.

Interpretation: the project was left after a broad browser/platform integration push, not obviously halfway through one small bounded feature.

## What is already present in Brazen

The current repository contains substantial infrastructure that should not be casually rewritten:

- native `egui` / `eframe` desktop shell,
- `BrowserEngine` abstraction,
- session, tab/window, navigation, history, zoom, and recovery concepts,
- configuration/bootstrap/runtime-path handling,
- profile persistence,
- capability-oriented permissions,
- logging and audit-related infrastructure,
- cache/asset plane,
- extraction helpers,
- WebSocket automation server,
- CLI introspection/control client,
- DOM/tab/window automation surfaces,
- event subscriptions and rate/backpressure concepts,
- MCP and virtual-resource seams,
- optional Servo integration.

The automation roadmap claims live support for tab enumeration/manipulation, DOM querying, screenshots, log streaming, event subscriptions, client authentication, capability checks, and backpressure. These are prime reuse candidates for the control-plane goal.

## External capability discovered during re-baselining: Codex App Server

A major scope-changing fact was confirmed on 2026-09-16: OpenAI exposes the full Codex harness through the official **Codex App Server** and describes it as the first-class integration path for rich clients.

App Server already owns functionality Brazen previously might have expected to build or broker itself:

- Codex agent loop,
- thread creation/resume/fork/archive and persistence,
- authentication/configuration/model discovery,
- tool execution and extensions,
- rich turn/item lifecycle,
- streaming UI-ready events,
- diffs,
- server-initiated approval requests,
- backward-compatible client protocol semantics.

Local clients normally launch a Codex/App Server binary as a long-running child process and communicate bidirectionally over JSON-RPC-lite framed as JSONL on stdio. OpenAI's own desktop and IDE clients use the same architectural boundary.

### Consequence

This **shrinks Brazen's legitimate scope**.

Brazen should not build:

- a second Codex agent runtime,
- a second Codex thread store,
- a custom Codex diff/approval/tool protocol,
- a replacement Codex Desktop UI,
- desktop-window automation as the normal Codex integration path.

The Codex side should be a thin adapter over App Server.

Brazen's distinct work remains:

- make ChatGPT Web a routable endpoint,
- bind ChatGPT conversations to Codex threads,
- route cross-surface messages/events,
- provide allow/hold/block policy,
- queue and release held traffic,
- preserve provenance,
- visualize topology and health,
- measure the wire,
- reconstruct causal history after failures.

This is not merely an implementation optimization. It is the current product boundary.

## Documentation problem

The old project model split work across many orthogonal roadmaps: shell UX, Servo, sessions, permissions, security, connectors, automation, cache, extraction, knowledge, media/TTS, persistence, observability, and virtual resources.

That was useful as capability inventory but harmful as an active execution model. It made nearly the entire browser platform look simultaneously unfinished.

From this point forward:

- `docs/roadmap.md` is the **single active product roadmap**.
- `docs/roadmaps/` is retained as historical/capability reference.
- A legacy roadmap item is not active merely because its checkbox is unchecked.
- App Server-owned Codex functionality is not a Brazen backlog item merely because Brazen may want to display or route around it.

## Known inconsistencies / hygiene hazards

The recovered tree has signs of development sprawl that should be handled deliberately rather than interpreted as product direction:

- tracked root log files such as `run.log`, `browser.log`, and `brazen_stdout.log`,
- recovery/patch artifacts such as `src/app.rs.orig`, `src/app.rs.rej`, and `src/automation.rs.bak`,
- roadmap checkboxes that are stale or inconsistent with the current README and code surface,
- a very broad checked-in roadmap coverage file,
- vendored/optional Servo complexity that can consume re-entry time.

Do **not** delete recovery artifacts merely for tidiness until their usefulness has been checked. Prevent new generated junk from accumulating, then remove confirmed-dead artifacts in a bounded hygiene task.

## Active product direction

Brazen is a local visual control plane for AI work-surface connections.

First supported wire:

**ChatGPT Web <-> Codex App Server**

The first product milestone is defined in `docs/milestones/001-chatgpt-codex-control-plane.md`.

Target operator experience:

- multiple ChatGPT conversations,
- selectable Codex App Server threads,
- explicit persisted pairings,
- live visual connections,
- inspectable bidirectional routed traffic,
- traffic rate/size/queue metrics,
- independent **allow / hold / block** policy for each direction,
- durable attribution after failures.

## Codex integration facts vs unknowns

### Known

- App Server is an official, client-oriented Codex integration boundary.
- It exposes rich Codex thread/turn/item state and bidirectional requests/events.
- Local clients can launch App Server as a child process and keep a long-lived stdio channel open.
- Protocol schemas can be generated from the Codex tooling.

### Still must be verified locally

- exact installed App Server version and current schema,
- Windows lifecycle/transport behavior on this machine,
- whether a Brazen-owned App Server process sees the same persisted threads as Codex Desktop,
- whether live simultaneous thread correlation/attachment with the separately running desktop app is possible or desirable,
- which native App Server events should be normalized as operator-visible status vs eligible cross-surface routed messages.

Direct attachment to the already-running Codex Desktop process is **not** required for Milestone 1. A Brazen-owned App Server instance is acceptable if it provides the necessary Codex thread workflow.

## Re-entry gate before feature implementation

Before the next major implementation task:

1. clone/update the branch on the actual Windows development machine,
2. record toolchain versions,
3. run the existing standard checks/build,
4. launch the current shell and capture what actually works,
5. verify whether a normal ChatGPT page can load, authenticate, render, accept input, and maintain a session through the current engine path,
6. verify the actual current multi-tab behavior instead of trusting stale roadmap checkboxes,
7. launch/inspect Codex App Server and generate or inspect its current schema,
8. record current thread/turn/item/approval primitives and local lifecycle behavior,
9. test thread visibility/correlation with Codex Desktop without making it a product dependency,
10. update this file with verified results.

Do not begin by fixing unrelated Servo bugs, building a custom Codex runtime, or reviving old capability tracks.
