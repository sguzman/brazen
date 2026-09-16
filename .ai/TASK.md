# Active Task — Re-entry Evidence Pass

Status: **READY, NOT YET RUN**

Owner: Codex implementer / grunt

This is the next task to run when active implementation resumes. Do not begin Milestone 1 feature work before producing this evidence.

## Objective

Establish the actual September 2026 state of Brazen on the user’s Windows development machine and remove uncertainty left by stale documentation.

This is primarily an inspection/verification task. Make only minimal fixes required to run an existing check or launch path; do not redesign the browser or begin the control-plane feature.

## Required work

1. Read `AGENTS.md`, `docs/PRODUCT.md`, `docs/CURRENT_STATE.md`, `docs/roadmap.md`, and `docs/milestones/001-chatgpt-codex-control-plane.md`.
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
7. Smoke-test the locally installed Codex App Server interface:
   - confirm invocation/version,
   - generate or inspect current protocol/schema if available,
   - identify supported thread list/start/resume/read/send/event primitives,
   - record how streaming events and approvals appear,
   - investigate whether a client can discover/attach to/correlate with threads already visible in the separately running Codex desktop app.
8. Inspect tracked recovery/log artifacts (`*.orig`, `*.rej`, `*.bak`, root `*.log`) and classify each as `retain-for-recovery`, `safe-to-remove`, or `unknown`. Do not delete unknown artifacts.
9. Update `docs/CURRENT_STATE.md` with verified facts.
10. Write `.ai/REPORT.md` with results and blockers.

## Prohibited scope

- no general Servo compatibility campaign,
- no knowledge/TTS/cache feature work,
- no generalized connector framework,
- no large refactor for cleanliness,
- no silent deletion of recovery artifacts,
- no control-plane UI implementation yet,
- no assumption that Codex desktop attachment exists unless demonstrated.

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
| Codex thread primitives | verified list | protocol evidence |
| Existing Codex desktop thread attachment/correlation | yes/no/unknown | evidence |

Finish by recommending the **smallest next implementation task** for Milestone 1. Do not implement that next task in the same run unless explicitly instructed.
