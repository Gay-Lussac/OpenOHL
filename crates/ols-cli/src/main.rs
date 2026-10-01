// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols` — OLS CAD command-line interface.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ols",
    version,
    about = "OLS CAD — Open Line System Computer Assisted Design"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print version and build information.
    Info,
    /// Run validation cases and report against their reference results.
    Validate {
        /// Case file or directory of case files.
        #[arg(default_value = "benchmarks")]
        path: PathBuf,
        /// Also write the report as Markdown to this file.
        #[arg(long)]
        report: Option<PathBuf>,
        /// List the calculations available to validation cases and exit.
        #[arg(long)]
        list: bool,
    },
    /// Inspect plugins.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Load a WebAssembly plugin from its directory and print what it reports.
    Info {
        /// Directory containing `ols-plugin.toml`.
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Info) | None => {
            print_info();
            ExitCode::SUCCESS
        }
        Some(Command::Validate { path, report, list }) => validate(&path, report.as_deref(), list),
        Some(Command::Plugin {
            command: PluginCommand::Info { dir },
        }) => plugin_info(&dir),
    }
}

fn print_info() {
    println!("OLS CAD {}", env!("CARGO_PKG_VERSION"));
    println!("core    {}", ols_core::VERSION);
    println!("license MPL-2.0");
}

fn plugin_info(dir: &std::path::Path) -> ExitCode {
    let result = ols_plugins::WasmHost::new().and_then(|host| {
        let mut plugin = host.load(dir)?;
        let info = plugin.describe()?;
        Ok((plugin.manifest().clone(), info))
    });
    match result {
        Ok((manifest, info)) => {
            println!("name         {}", info.name);
            println!("version      {}", info.version);
            println!("description  {}", info.description);
            println!("tier         {}", manifest.plugin.tier);
            println!("sdk          {}", manifest.plugin.sdk);
            println!("entry        {}", manifest.entry_path().display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn validate(path: &std::path::Path, report_path: Option<&std::path::Path>, list: bool) -> ExitCode {
    use ols_validation::{Outcome, Registry, Report, load_cases};

    let registry = Registry::with_builtin();
    if list {
        for calculation in registry.iter() {
            println!("{:<24} {}", calculation.id(), calculation.description());
        }
        return ExitCode::SUCCESS;
    }

    let cases = match load_cases(path) {
        Ok(cases) => cases,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let report = Report::run(&registry, cases);

    for result in &report.results {
        let case = &result.case.case;
        let mark = if result.passed() { "ok  " } else { "FAIL" };
        println!(
            "{mark} {:<16} {:>9.1?}  {}",
            case.id, result.elapsed, case.title
        );
        if !result.passed() {
            match &result.outcome {
                Outcome::Error(message) => println!("       error: {message}"),
                Outcome::TimedOut(limit) => println!("       exceeded time limit of {limit:?}"),
                Outcome::Ran(checks) => {
                    for check in checks.iter().filter(|c| !c.passed()) {
                        let actual = check.actual.map_or("missing".into(), |a| a.to_string());
                        println!(
                            "       {}: expected {}, got {actual}",
                            check.name, check.expected.value
                        );
                    }
                }
            }
        }
    }
    println!(
        "\n{} cases: {} passed, {} failed",
        report.results.len(),
        report.passed(),
        report.failed()
    );

    if let Some(report_path) = report_path
        && let Err(error) = std::fs::write(report_path, report.to_markdown())
    {
        eprintln!("error: cannot write {}: {error}", report_path.display());
        return ExitCode::from(2);
    }

    if report.all_passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
