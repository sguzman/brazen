# Active Task — Re-entry Evidence Pass

Status: **READY, NOT YET RUN**

Owner: Codex implementer / grunt

This is the next task to run when active implementation resumes. Do not begin Milestone 1 feature work before producing this evidence.

## Objective

Establish the actual September 2026 state of Brazen on the user's Windows development machine and verify the two endpoint boundaries that now define Milestone 1:

- ChatGPT Web through Brazen's browser surface,
- Codex through the official App Server.

This is primarily an inspection/verification task. Make only minimal fixes required to run an existing check or launch path. Do not redesign the browser, build a Codex runtime, or begin the control-plane feature.

## Important architectural fact

OpenAI's Codex App Server already exposes the Codex harness as a rich client protocol. Treat it as substrate, not inspiration.

Do **not** propose or implement replacements for:

- Codex agent loop,
- thread persistence,
- auth/config/model discovery,
- tool execution,
- turn/item lifecycle,
- diff semantics,
- approval semantics,
- Codex-native event protocol.

The purpose of the App Server investigation is to learn how thin Brazen's Codex adapter can be.

## Required work

1. Read `AGENTS.md`, `docs/PRODUCT.md`, `docs/CURRENT_STATE.md`, `docs/roadmap.md`, `docs/decisions/0001-control-plane-first.md`, `docs/decisions/0002-adopt-codex-app-server.md`, and `docs/milestones/001-chatgpt-codex-control-plane.md`.
2. Record relevant environment/toolchain versions.
3. Run the existing build/test/check surface available in the repository.
4. Launch the current Brazen shell.
5. Record what currently works for tabs/windows/navigation.
6. Test ChatGPT through the current browser engine path:
   - page load,
   - authentication/login state if already available,
   - rendering,
   - text input,
   - message submission,
   - scrolling,
   - session persistence across ordinary relaunch,
   - opening/maintaining at least two ChatGPT conversations if current tab support permits it.
7. Inspect the locally installed Codex App Server boundary:
   - confirm `codex` and App Server version/invocation,
   - run `codex app-server generate-json-schema` or the current equivalent if supported,
   - preserve a compact summary of the generated/current protocol surface,
   - identify thread start/list/resume/read/fork/archive primitives,
   - identify turn/input primitives,
   - identify thread/turn/item streaming notifications,
   - identify server-initiated approval request/response primitives,
   - identify model/config/auth surfaces Brazen should consume rather than duplicate,
   - record process lifecycle and transport behavior on Windows,
   - investigate whether a Brazen-owned App Server process can discover/resume/correlate with threads visible in the separately running Codex desktop app.
8. Classify App Server events into at least three conceptual buckets for future design:
   - `endpoint status/telemetry`,
   - `operator action/approval`,
   - `candidate cross-surface routed content`.
   Do not assume every App Server delta/event should become traffic on the Brazen wire.
9. Inspect tracked recovery/log artifacts (`*.orig`, `*.rej`, `*.bak`, root `*.log`) and classify each as `retain-for-recovery`, `safe-to-remove`, or `unknown`. Do not delete unknown artifacts.
10. Update `docs/CURRENT_STATE.md` with verified facts.
11. Write `.ai/REPORT.md` with results and blockers.

## Prohibited scope

- no general Servo compatibility campaign,
- no knowledge/TTS/cache feature work,
- no generalized connector framework,
- no large refactor for cleanliness,
- no silent deletion of recovery artifacts,
- no control-plane UI implementation yet,
- no custom Codex agent/session/runtime implementation,
- no duplicate Codex chat UI,
- no Codex Desktop GUI automation unless needed solely as an experiment,
- no assumption that live attachment to the Desktop app exists unless demonstrated.

## Acceptance criteria

The report must contain a compact compatibility table covering:

| Surface | Result | Evidence / notes |
| --- | --- | --- |
| Existing build | pass/fail | command + failure if any |
| Existing tests | pass/fail | command + failure if any |
| Brazen shell launch | pass/fail | observed behavior |
| Current multi-tab behavior | verified state | what actually works |
| ChatGPT load/render | pass/fail | observed behavior |
| ChatGPT input/send | pass/fail | observed behavior |
| ChatGPT session persistence | pass/fail/unknown | observed behavior |
| Codex App Server launch | pass/fail | version/invocation |
| Schema generation/inspection | pass/fail | command/artifact/notes |
| Codex thread primitives | verified list | protocol evidence |
| Turn/item/event primitives | verified list | protocol evidence |
| Approval primitives | verified list | protocol evidence |
| App Server process lifecycle on Windows | verified state | observed behavior |
| Existing Codex Desktop thread visibility/correlation | yes/no/unknown | evidence |

Finish by recommending the **smallest next implementation task** for Milestone 1. Prefer a thin App Server client spike or ChatGPT endpoint spike based on whichever boundary is least certain. Do not implement that next task in the same run unless explicitly instructed.
