# Task runner — thin launchers, logic lives in nushell (scripts/).

default:
    @just --list

# Build all workspace crates (debug profile).
build:
    cargo build --workspace

# Build and install the mulch CLI globally (self-hosting cutover target;
# until then the bootstrap `ml` CLI stays the expertise surface). cargo's
# bin dir is on PATH via rustup.
install: build
    cargo install --path crates/mulch --locked --force

# Touched-crates quality gate (the deterministic tester step calls this).
qualitygate:
    nu scripts/qualitygate.nu

# Stage-scoped verification dispatcher; `just verify implementer` is the
# implementer's one mechanical verification call.
verify stage:
    nu scripts/verify.nu {{ stage }}

# Run a workflow end to end: create+start+attach, wait, integrate
# (thin wrapper — logic lives in scripts/run_workflow.nu; GitButler-
# adapted: integration via `but branch update main`).
run *args:
    nu scripts/run_workflow.nu {{ args }}
