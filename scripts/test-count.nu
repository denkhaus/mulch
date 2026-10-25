# The workspace test-count ledger (retro 2026-10-25): `cargo nextest
# list` is the fast (~1s) deterministic count the loop compares at
# closeout against `.iterate-state.json`'s `last_test_count` — a drop
# without an announced removal, or a commit message claiming tests the
# tree does not carry, stops the closeout (it caught both the sprint-49
# checkout incident and the rebase ours/theirs loss).
def main [] {
    let count = (cargo nextest list --workspace | lines | length)
    print $count
}
