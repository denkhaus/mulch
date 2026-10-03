# GitHub App Workflows permission (operator guide)

The develop loop's planner preflight refuses to route seeds that touch
`.github/workflows/**` while the fabro GitHub App cannot push workflow
files (incident run 01M32J3AF: every push rejected with *"refusing to
allow a GitHub App to create or update workflow ... without workflows
permission"*). To make that verdict deterministic, the loop reads an
**operator marker file**:

```
.fabro/github-app-workflows-permission
```

## What the marker is

A one-word file at the repo root, maintained by the operator, stating
the fabro GitHub App's **effective** Workflows permission:

| Value      | Meaning |
|------------|---------|
| `granted`  | the App's pushes can create/update workflow files |
| `absent`   | the App's pushes are rejected for workflow files |

It is the **primary, deterministic source** for the preflight's
`publish_blocked_risk` arm: `.fabro/workflows/develop/scripts/planner-preflight.nu`
resolves the permission from it (env flag and a read-only `gh` installation
probe are conveniences layered on top; the resolution order only decides
which convenience wins when several are present). With `absent`, the
planner routes workflow-targeting seeds as Blocked at claim time instead
of letting them fail mid-push. `unknown` (no source answers) never flags
— the arm fails open.

## Current value and why

The marker is currently **`absent`**. The fabro App itself holds
workflows read/write, but the engine-minted installation tokens omit the
workflows permission (fabro-11d9 lineage, denkhaus/fabro) — so the
*effective* permission for loop pushes today is `absent`.

## Staleness contract

The marker **must flip to `granted` in the same deploy that ships
fabro-11d9** (engine-minted tokens gaining the workflows permission).
Do not flip it earlier (the loop would route workflow seeds into pushes
that still fail) and do not flip it later (workflow-targeting seeds stay
unnecessarily blocked). When flipping, also revisit the image-release
runbook below.

## Tie-in: image-release runbook

The toolchain image bakes in this repo's tracker and tooling, so a
permission flip (or any marker change worth propagating) is a natural
reminder to rebuild/repin the run-sandbox image via the `just
image-release` runbook (seeds-b56f follow-up; seeds-6eb8 executes it).
Keep this page and that runbook in the same review when either changes.
