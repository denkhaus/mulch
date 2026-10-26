//! `mulch setup` — parity with the reference 0.10.7 (setup.ts
//! `registerSetupCommand`). The recipe layer (git hooks, built-in
//! providers, discovery) lives in [`super::setup_recipes`].
//!
//! Probed channel laws (ml 0.10.7, /tmp/sprint55/setup/probes):
//!
//! - Error surfaces (`.mulch/` missing, no provider and no `--hooks`, unknown
//!   provider, a resolution throw) render `Error: <message>` on stderr plain,
//!   the error envelope on STDERR in `--json`, exit 1. The no-provider pair is
//!   deliberately divergent: plain lowercases after the prefix (`Error: specify
//!   a provider, …`), json capitalizes (`Specify a provider, …`).
//! - Result surfaces (a recipe's `RecipeResult`, `--hooks` runs) print their
//!   envelope on STDOUT in `--json` — success and failure alike — and plain
//!   follows the glyph laws: `✔ <msg>` stdout on success; `✖ <msg>` stderr for
//!   hooks and for install/remove failures (prefixed `Error: ` for the latter);
//!   `✖ <msg>` STDOUT for a `--check` failure (the reference's yellow
//!   console.log; this port never emits ANSI — the piped bytes are identical).
//! - Exit code 1 is sticky: a failing `--hooks` pass sets it, the provider pass
//!   still runs, and a later success does not clear it (the reference's
//!   `process.exitCode` law).
//!
//! DEVIATIONS (README): every generated integration names THIS
//! binary (`mulch prime` where the reference writes `ml prime`; the
//! codex toml fence says `mulch setup codex`), the unknown-provider
//! hint says `mulch setup --list`, and the provider-argument help is
//! native-honest (no npm recipe mention). This port renders no ANSI
//! color under any environment.

use std::path::Path;

use serde_json::{Map, Value, json};

use super::setup_recipes::{
    Action, NPM_RECIPE_PREFIX, ProviderListing, RecipeResult, RecipeSource, check_git_hook,
    install_git_hook, list_filesystem_recipes, remove_git_hook, resolve_recipe,
};
use crate::cli::{GlobalOpts, SetupArgs};
use crate::output::{Failure, failure_envelope, print_json, success_envelope};

/// The built-in provider names, sorted (setup.ts 603).
const BUILTIN_PROVIDER_NAMES: [&str; 3] = ["claude", "codex", "cursor"];

/// Runs `setup` (setup.ts 624-786).
pub(super) fn run(opts: &GlobalOpts, args: &SetupArgs) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::error_line("setup", format!("resolving cwd: {source}")))?;

    // Verify .mulch/ exists — a file counts too (existsSync law).
    if !cwd.join(".mulch").exists() {
        return Err(Failure::error_line(
            "setup",
            "No .mulch/ directory found. Run `mulch init` first.",
        ));
    }

    // --list needs no provider (setup.ts 661-664).
    if args.list {
        run_list(opts, &cwd);
        return Ok(());
    }

    if args.provider.is_none() && !args.hooks {
        // The divergent pair (probed): plain lowercases after the
        // prefix, json capitalizes.
        return Err(Failure::divergent(
            "setup",
            "Error: specify a provider, use --hooks, or use --list.",
            "Specify a provider, use --hooks, or use --list.",
        ));
    }

    // Exit-code stickiness: a failing pass sets the flag; the final
    // conversion owns the exit (process.exitCode law).
    let mut failed = false;

    // --hooks (setup.ts 677-709).
    if args.hooks {
        let action = resolve_action(args);
        // The git-hook helpers convert their own io failures into
        // throw strings; the reference would crash on those (an
        // unhandled rejection outside the recipe dispatch) — ours
        // renders them as the pass's failure result (the crash-path
        // deviation family: same exit, clean text).
        let result = match action {
            Action::Check => check_git_hook(&cwd),
            Action::Remove => remove_git_hook(&cwd),
            Action::Install => install_git_hook(&cwd),
        }
        .unwrap_or_else(RecipeResult::fail);
        let mut fields = Map::new();
        fields.insert("target".into(), json!("hooks"));
        fields.insert("action".into(), json!(action.as_str()));
        fields.insert("message".into(), json!(result.message));
        report_result(opts, args, None, &result, &fields);
        failed |= !result.success;
        // No provider: stop after the hooks pass.
        if args.provider.is_none() {
            return exit(failed);
        }
    }

    let Some(provider) = args.provider.as_deref() else {
        return exit(failed);
    };

    // Provider resolution (setup.ts 715-738).
    let resolved = match resolve_recipe(provider, &cwd) {
        Ok(Some(resolved)) => resolved,
        Ok(None) => {
            // The hint names an invocation of THIS binary (promote
            // law; the reference says `ml setup --list`).
            let hint = format!(
                "Unknown provider \"{provider}\". Run `mulch setup --list` to see discovered \
                 providers, or add a recipe at .mulch/recipes/{provider}.{{ts,sh}} or install \
                 {NPM_RECIPE_PREFIX}{provider}."
            );
            return Err(Failure::error_line("setup", hint));
        }
        Err(throw) => return Err(Failure::error_line("setup", throw)),
    };

    // Action dispatch + the throw conversion (setup.ts 740-762).
    let action = resolve_action(args);
    let result = match resolved.recipe.run(action, &cwd) {
        Ok(result) => result,
        Err(throw) => {
            let label = match (&resolved.source, &resolved.path) {
                (RecipeSource::Builtin, _) => "built-in".to_string(),
                (_, Some(path)) => path.display().to_string(),
                (_, None) => resolved.source.as_str().to_string(),
            };
            threw_result(provider, action, &label, &throw)
        }
    };

    let mut fields = Map::new();
    fields.insert("provider".into(), json!(provider));
    fields.insert("source".into(), json!(resolved.source.as_str()));
    if let Some(path) = &resolved.path {
        fields.insert("path".into(), json!(path.display().to_string()));
    }
    fields.insert("action".into(), json!(action.as_str()));
    fields.insert("message".into(), json!(result.message));
    report_result(opts, args, Some(provider), &result, &fields);
    failed |= !result.success;
    exit(failed)
}

/// `options.check ? "check" : options.remove ? "remove" : "install"`
/// (setup.ts 680/740) — check wins when both are set.
fn resolve_action(args: &SetupArgs) -> Action {
    if args.check {
        Action::Check
    } else if args.remove {
        Action::Remove
    } else {
        Action::Install
    }
}

/// The threw conversion (setup.ts 750-762): a recipe that throws
/// surfaces as a one-line failure, not a stack trace.
fn threw_result(provider: &str, action: Action, label: &str, throw: &str) -> RecipeResult {
    RecipeResult::fail(format!(
        "recipe \"{provider}\" {} threw ({label}): {throw}",
        action.as_str()
    ))
}

/// Renders one result pass: the json envelope on STDOUT, or the plain
/// glyph laws — `✔` stdout on success; `✖` stderr for hooks and
/// install/remove failures (the latter prefixed `Error: `); `✖`
/// STDOUT for a `--check` failure (setup.ts 689-701, 764-780). No
/// ANSI ever (the repo's no-colour law).
fn report_result(
    opts: &GlobalOpts,
    args: &SetupArgs,
    provider: Option<&str>,
    result: &RecipeResult,
    fields: &Map<String, Value>,
) {
    if opts.json {
        let envelope = if result.success {
            success_envelope("setup", fields.clone())
        } else {
            failure_envelope("setup", fields.clone())
        };
        print_json(&envelope, false);
        return;
    }
    if result.success {
        print_success_glyph(&format!("✔ {}", result.message));
    } else if args.check && provider.is_some() {
        // A provider --check failure is the yellow stdout arm.
        print_success_glyph(&format!("✖ {}", result.message));
    } else if provider.is_some() {
        print_failure_line(&format!("Error: {}", result.message));
    } else {
        // Hooks failures carry no Error: prefix (setup.ts 700).
        print_failure_line(&format!("✖ {}", result.message));
    }
}

/// stdout glyph (the green console.log arm).
#[allow(
    clippy::print_stdout,
    reason = "result rendering is this command's job"
)]
fn print_success_glyph(text: &str) {
    println!("{text}");
}

/// stderr line (the red console.error arm).
#[allow(
    clippy::print_stderr,
    reason = "result rendering is this command's job"
)]
fn print_failure_line(text: &str) {
    eprintln!("{text}");
}

/// `runList` (setup.ts 847-887): the provider table. Ours lists
/// built-ins plus filesystem recipes (relative paths), sorted by
/// name then filesystem-before-builtin — the npm `shadowedBy`
/// probing is a documented deviation (absent).
fn run_list(opts: &GlobalOpts, cwd: &Path) {
    let mut listings = list_filesystem_recipes(cwd);
    for name in BUILTIN_PROVIDER_NAMES {
        listings.push(ProviderListing {
            name:   name.to_string(),
            source: RecipeSource::Builtin,
            path:   None,
        });
    }
    listings.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.source.list_order().cmp(&b.source.list_order()))
    });

    if opts.json {
        let providers: Vec<Value> = listings
            .iter()
            .map(|listing| {
                let mut row = Map::new();
                row.insert("name".into(), json!(listing.name));
                row.insert("source".into(), json!(listing.source.as_str()));
                if let Some(path) = &listing.path {
                    row.insert("path".into(), json!(relative_display(path, cwd)));
                }
                Value::Object(row)
            })
            .collect();
        let mut fields = Map::new();
        fields.insert("action".into(), json!("list"));
        fields.insert("providers".into(), Value::Array(providers));
        print_json(&success_envelope("setup", fields), false);
        return;
    }

    print_success_glyph("Available providers:");
    let label_width = listings
        .iter()
        .map(|listing| listing.name.len())
        .max()
        .unwrap_or(0)
        .max(6);
    for listing in &listings {
        let source_label = match (&listing.source, &listing.path) {
            (RecipeSource::Builtin, _) => "built-in".to_string(),
            (source, Some(path)) => format!("{}: {}", source.as_str(), relative_display(path, cwd)),
            (source, None) => source.as_str().to_string(),
        };
        print_success_glyph(&format!(
            "  ✓ {:<width$}  {}",
            listing.name,
            source_label,
            width = label_width
        ));
    }
    // The blank line between table and footer (setup.ts 881).
    print_success_glyph("");
    // Native-honest footer (deviation): this port resolves
    // filesystem .sh → built-in; .ts and npm recipes error.
    print_success_glyph(
        "Resolution order: filesystem (.mulch/recipes/<name>.sh) → built-in. TypeScript and npm \
         recipes are not supported by this native build.",
    );
}

/// `relative(cwd, path)` — a path under cwd renders repo-relative,
/// anything else absolute (Node's path.relative is identity for
/// non-prefix paths only when equal; foreign absolute paths render
/// as `../…` there — ours keeps the absolute form, a wording-level
/// nicety for a case the recipes never produce).
fn relative_display(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).map_or_else(
        |_| path.display().to_string(),
        |relative| relative.display().to_string(),
    )
}

/// The sticky exit: everything is already rendered; only the code
/// remains (the rendered-exit law, mulch-230f).
fn exit(failed: bool) -> Result<(), Failure> {
    if failed {
        Err(Failure::rendered_exit("setup"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(check: bool, remove: bool) -> SetupArgs {
        SetupArgs {
            provider: None,
            check,
            remove,
            hooks: false,
            list: false,
        }
    }

    /// The action table (setup.ts 680/740): check wins over remove,
    /// remove over install.
    #[test]
    fn resolve_action_prefers_check_then_remove() {
        assert_eq!(resolve_action(&args(false, false)), Action::Install);
        assert_eq!(resolve_action(&args(false, true)), Action::Remove);
        assert_eq!(resolve_action(&args(true, false)), Action::Check);
        assert_eq!(resolve_action(&args(true, true)), Action::Check);
    }

    /// The threw wrapper's shape (setup.ts 758-761): the recipe name,
    /// the action, the source label in parentheses, the message.
    #[test]
    fn threw_result_wraps_with_the_reference_shape() {
        assert_eq!(
            threw_result("claude", Action::Install, "built-in", "boom").message,
            "recipe \"claude\" install threw (built-in): boom"
        );
        assert_eq!(
            threw_result("helper", Action::Check, "/x/helper.sh", "boom").message,
            "recipe \"helper\" check threw (/x/helper.sh): boom"
        );
    }

    /// `relative_display`: under-cwd paths relativize, foreign paths
    /// stay absolute.
    #[test]
    fn relative_display_strips_the_cwd_prefix() {
        let cwd = Path::new("/repo");
        assert_eq!(
            relative_display(Path::new("/repo/.mulch/recipes/a.sh"), cwd),
            ".mulch/recipes/a.sh"
        );
        assert_eq!(
            relative_display(Path::new("/elsewhere/a.sh"), cwd),
            "/elsewhere/a.sh"
        );
    }
}
