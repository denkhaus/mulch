//! Binary entry point: argument parsing, command dispatch, exit codes.
//!
//! The format core lives in the library crate; this binary owns only
//! the command surface (`mulch-da8b` parity slice, reference `ml
//! 0.10.7`). Help honesty: the command tree lists ONLY implemented
//! commands.

mod cli;
mod commands;
mod output;

use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory as _, Parser as _};
use cli::Cli;

fn main() -> ExitCode {
    let started = std::time::Instant::now();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return render_parse_failure(&error),
    };
    let Some(command) = cli.command.as_ref() else {
        if cli.version {
            #[allow(clippy::print_stdout, reason = "version renders on stdout")]
            {
                println!(env!("CARGO_PKG_VERSION"));
            }
            return ExitCode::SUCCESS;
        }
        // The reference's no-args contract: full help on stderr, exit 1.
        #[allow(clippy::print_stderr, reason = "help renders on stderr here")]
        {
            eprintln!("{}", Cli::command().render_help());
        }
        return ExitCode::from(output::EXIT_ERROR);
    };
    if cli.version {
        // The reference prints the bare version value (our version
        // value itself is a documented deviation).
        #[allow(clippy::print_stdout, reason = "version renders on stdout")]
        {
            println!(env!("CARGO_PKG_VERSION"));
        }
        return ExitCode::SUCCESS;
    }
    let timing = cli.opts.timing;

    let outcome = commands::dispatch(&cli, command);
    if timing {
        report_timing(started);
    }
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            output::render_failure(&failure, cli.opts.json, failure.envelope_to_stderr);
            ExitCode::from(failure.code)
        }
    }
}

/// Prints the reference's `Done in Nms` timing line to stderr.
#[allow(clippy::print_stderr, reason = "timing contract renders on stderr")]
fn report_timing(started: std::time::Instant) {
    let millis = started.elapsed().as_millis();
    eprintln!("Done in {millis}ms");
}

/// Maps clap parse failures onto the reference's exit-1 contract:
/// no arguments print the help to STDERR, unknown commands print the
/// two-line unknown hint; `--help` keeps clap's stdout/exit-0 shape.
fn render_parse_failure(error: &clap::Error) -> ExitCode {
    match error.kind() {
        ErrorKind::DisplayHelp => {
            #[allow(clippy::print_stdout, reason = "help renders on stdout")]
            {
                println!("{}", error.render());
            }
            ExitCode::SUCCESS
        }
        ErrorKind::InvalidSubcommand => {
            #[allow(clippy::print_stderr, reason = "error rendering is the CLI boundary")]
            {
                eprintln!("Unknown command: {}", unknown_command(error));
                eprintln!("Run 'mulch --help' for usage.");
            }
            ExitCode::from(output::EXIT_ERROR)
        }
        _ => {
            #[allow(clippy::print_stderr, reason = "error rendering is the CLI boundary")]
            {
                eprintln!("{}", error.render());
            }
            ExitCode::from(output::EXIT_ERROR)
        }
    }
}

/// Extracts the offending command name from clap's invalid-subcommand
/// error (the first context string).
fn unknown_command(error: &clap::Error) -> String {
    error
        .get(clap::error::ContextKind::InvalidSubcommand)
        .and_then(|value| value.to_string().split(' ').next().map(String::from))
        .unwrap_or_else(|| "<unknown>".into())
}
