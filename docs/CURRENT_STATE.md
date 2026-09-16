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

Interpretation: the project was not obviously abandoned halfway through one small feature. It was left after a broad browser/platform integration push.

## What is already present

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

The automation roadmap claims live support for tab enumeration/manipulation, DOM querying, screenshots, log streaming, event subscriptions, client authentication, capability checks, and backpressure. These are prime reuse candidates for the new control-plane goal.

## Documentation problem

The old project model split work across many orthogonal roadmaps: shell UX, Servo, sessions, permissions, security, connectors, automation, cache, extraction, knowledge, media/TTS, persistence, observability, and virtual resources.

That was useful as a capability inventory but harmful as an active execution model. It made nearly the entire browser platform look simultaneously unfinished.

From this point forward:

- `docs/roadmap.md` is the **single active product roadmap**.
- `docs/roadmaps/` is retained as historical/capability reference.
- A legacy roadmap item is not active merely because its checkbox is unchecked.

## Known inconsistencies / hygiene hazards

The recovered tree has signs of development sprawl that should be handled deliberately rather than interpreted as product direction:

- tracked root log files such as `run.log`, `browser.log`, and `brazen_stdout.log`,
- recovery/patch artifacts such as `src/app.rs.orig`, `src/app.rs.rej`, and `src/automation.rs.bak`,
- roadmap checkboxes that are stale or inconsistent with the current README and code surface,
- a very broad checked-in roadmap coverage file,
- vendored/optional Servo complexity that can consume re-entry time.

Do **not** delete recovery artifacts merely for tidiness until their usefulness has been checked. Prevent new generated junk from accumulating, then remove confirmed-dead artifacts in a bounded hygiene task.

## Active product direction

Brazen is being narrowed to a browser <-> local-agent coordination plane.

The first product milestone is defined in `docs/milestones/001-chatgpt-codex-control-plane.md`.

The target operator experience is:

- multiple ChatGPT tabs,
- local Codex threads,
- explicit ChatGPT <-> Codex pairings,
- live visual connections,
- inspectable bidirectional traffic,
- traffic rate/size/queue metrics,
- independent **allow / hold / block** policy for each direction,
- durable attribution after failures.

## Codex integration assumption

The preferred first integration target is the official **Codex App Server** boundary rather than desktop-window automation. The App Server is a bidirectional JSON-RPC/JSONL-over-stdio interface intended for local rich clients.

However, one fact must be verified during implementation: whether Brazen can attach to or correlate with threads already owned by a separately running Codex desktop app. Do not assume that direct desktop-process attachment exists.

If direct attachment is unavailable, Brazen may host its own App Server process while preserving a clean adapter boundary. The milestone is about removing the human relay burden, not about depending on UI automation of the Codex desktop window.

## Re-entry gate before feature implementation

Before the next major implementation task:

1. clone/update the branch on the actual Windows development machine,
2. record toolchain versions,
3. run the existing standard checks/build,
4. launch the current shell and capture what actually works,
5. verify whether a normal ChatGPT page can load, authenticate, render, accept input, and maintain a session through the current engine path,
6. verify the actual current multi-tab behavior instead of trusting stale roadmap checkboxes,
7. run an isolated Codex App Server smoke test and record available thread/session operations,
8. update this file with verified results.

Do not begin by fixing unrelated Servo bugs or reviving old capability tracks.
