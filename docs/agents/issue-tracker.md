# Issue tracker: Seeds (this repo's own `.seeds/` store)

Issues and specs for this repo live in the repo's own Seeds tracker
(git-native, `.seeds/`), driven by the `seeds` CLI — **not** GitHub
Issues (explicitly unused, see `AGENTS.md`). Seed ids carry the prefix
`mulch-`.

## Conventions

- **Create**: `seeds create --title "..." --type <task|bug|feature|epic> --priority <0-4> --desc "..."`. Filers file UNASSIGNED: never pass `--assignee` — new seeds land in the backlog; assignment is the operator's ownership switch.
- **Read one**: `seeds show <id> --format json` — the supported read path; never parse `.seeds/issues.jsonl` by hand.
- **List / queue**: `seeds ready --assignee fabro --limit 200` (unblocked, line-assigned); `seeds list --format json --assignee fabro --limit 200` for the full picture. Always `--limit 200` — the default 50 silently truncates.
- **Claim**: `seeds update <id> --status in_progress --assignee fabro`.
- **Amend body**: `seeds update <id> --description "<full corrected body>"` (replaces wholesale — re-emit the complete body).
- **Labels**: `--labels a,b` on create; `seeds update <id> --set-labels a,b` relabels.
- **Close**: never hand-close from a run — the deterministic Closeout step closes approved seeds; the planner's one exception is the superseded close with a mandatory `--reason`. Operator-surface sessions (no run) may close with a reason.
- **Dependencies**: `seeds dep add <issue> <depends-on>` / `seeds dep remove <issue> <depends-on>` / `seeds dep list <issue>`.
- **Search**: `seeds search "<keyword>" --format compact` (AND-strict — one keyword per query).

## Ownership

The develop line works EXCLUSIVELY on seeds assigned to `fabro`
(`--assignee fabro` on every list/ready call). Unassigned seeds
are the backlog; the user's veto is reassign or unassign.

## When a skill says "publish to the issue tracker"

Run `seeds create` (unassigned).

## When a skill says "fetch the relevant ticket"

Run `seeds show <id> --format json`.

## Wayfinding operations (for `/wayfinder`)

The map is a single seed; child tickets are seeds linked by dependencies.

- **Map**: one seed labelled `wayfinder:map` (`seeds create --labels wayfinder:map --title "..."`), body holding Notes / Decisions-so-far / Fog.
- **Child ticket**: `seeds create --labels wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`), first body line `Part of: <map-id>`; link to the map with `seeds dep add <child> <map>`.
- **Blocking**: `seeds dep add <child> <blocker>` (native tracker dependencies; `seeds ready` already excludes blocked seeds).
- **Frontier query**: `seeds ready --limit 200` (open + unblocked), drop assigned ones; first in priority order wins.
- **Claim**: `seeds update <id> --status in_progress --assignee <driver>`.
- **Resolve**: append the answer to the body (`seeds update <id> --description`), then close with a reason (`seeds close <id> --reason "wayfinder: <summary>"` — operator-surface close, see above); append the context pointer to the map body.
