# Task runner — thin launchers, logic lives in nushell (scripts/).

default:
    @just --list

# One-time checkout bootstrap (OPERATORS ONLY): trust + install the
# mise toolset (.mise.toml) and activate the lefthook pre-push gate.
# All three steps are idempotent. INVARIANT (lefthook.yml / AGENTS.md):
# run sandboxes NEVER install hooks — this recipe is the operator's
# explicit once-per-checkout opt-in; never call it from a run,
# toolchain image, or CI config.
bootstrap:
    mise trust
    mise install
    lefthook install

# Build all workspace crates (debug profile).
build:
    cargo build --workspace

# Build and install THIS repo's mulch CLI globally (the self-hosting
# install path, ADR-0007; the crates.io release feeds mise pins).
# cargo's bin dir is on PATH via rustup.
install: build
    cargo install --path crates/mulch --locked --force

# Touched-crates quality gate (the deterministic tester step calls this).
qualitygate:
    nu scripts/qualitygate.nu

# Stage-scoped verification dispatcher; `just verify implementer` is the
# implementer's one mechanical verification call.
verify stage:
    nu scripts/verify.nu {{ stage }}

# Rotate sprint notes: .iterate-state.json keeps the newest 10 notes,
# older ones move to .iterate-state-archive.jsonl (one JSON record per
# line, oldest first, append-only). Ported from brightctl 2026-10-17 —
# the growing notes list cost every session start a full-file read.
# Idempotent; the -dry variant previews the split without writing.
rotate-notes:
    nu scripts/rotate-sprint-notes.nu

rotate-notes-dry:
    nu scripts/rotate-sprint-notes.nu --dry-run

# The workspace test count (ledger: compare against .iterate-state.json's
# last_test_count at every closeout).
test-count:
    nu scripts/test-count.nu

# Run a workflow end to end: create+start+attach, wait, integrate
# (thin wrapper — logic lives in scripts/run_workflow.nu; integration
# via GitHub auto-merge + `git pull --ff-only`, mulch-e2df).
run *args:
    nu scripts/run_workflow.nu {{ args }}
