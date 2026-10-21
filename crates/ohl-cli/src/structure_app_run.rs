// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structure analyse|report`: a structure's results from the
//! structure application, run headless as a program of its own — not by
//! depending on its crates (a core crate never does). `ohl` starts
//! `ohl-structure-report --request -` (found as OHL-Design finds
//! OHL-Structure: the program [`PROGRAM_ENV`] names, else next to `ohl`
//! or in the folder above, else on the `PATH`), writes a JSON request on
//! its input and reads the JSON reply on its output
//! (`ohl-structure-request` / `ohl-structure-reply`, version 1;
//! `docs/development/data-model.md`).
//!
//! Exit clause: the tool is given `--timeout` seconds (default 300) and
//! stopped after them; its error output is shown when it fails.

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The structure side's headless tool.
pub const PROGRAM: &str = "ohl-structure-report";

/// The environment variable naming the tool (looked at first).
pub const PROGRAM_ENV: &str = "OHL_STRUCTURE_REPORT";

/// How the tool is built.
pub const BUILD: &str = "cargo build --release -p ohl-cli -p ohl-structure-export";

/// `format` of a request.
const REQUEST_FORMAT: &str = "ohl-structure-request";
/// `format` of a reply.
const REPLY_FORMAT: &str = "ohl-structure-reply";
/// The protocol's version.
const PROTOCOL_VERSION: u32 = 1;

/// What both subcommands take.
#[derive(clap::Args)]
pub struct StructureRunArgs {
    /// Structure file (.ohls).
    pub file: PathBuf,
    /// Cross-cross-section library files (.ohlsec) or folders; the first giving an id
    /// wins (default: the `Sections` folder beside the file, if any).
    #[arg(long, num_args = 1..)]
    pub sections: Vec<PathBuf>,
    /// A load table (CSV, as OHL-Structure writes it) instead of the
    /// file's own load cases evaluated without line values.
    #[arg(long)]
    pub loads: Option<PathBuf>,
    /// The extension to stand the structure on (default: as
    /// written).
    #[arg(long)]
    pub extension: Option<String>,
    /// A case to solve (repeatable; default: every case of the table).
    #[arg(long = "case")]
    pub cases: Vec<String>,
    /// Print the tool's JSON reply as it is.
    #[arg(long)]
    pub json: bool,
    /// Seconds the tool may take before it is stopped.
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

/// `ohl structure report`.
#[derive(clap::Args)]
pub struct StructureReportArgs {
    #[command(flatten)]
    pub run: StructureRunArgs,
    /// Report file (.html); the members' CSV goes beside it as
    /// `<stem>.members.csv`.
    #[arg(short, long)]
    pub output: PathBuf,
    /// Write today's date into the report.
    #[arg(long)]
    pub date: bool,
}

/// The request (`ohl-structure-request`, version 1).
#[derive(Debug, Serialize)]
struct Request<'a> {
    format: &'a str,
    version: u32,
    action: &'a str,
    structure: &'a Path,
    sections: &'a [PathBuf],
    loads: Option<&'a Path>,
    extension: Option<&'a str>,
    cases: &'a [String],
    report: Option<&'a Path>,
    date: bool,
}

/// One check of a case.
#[derive(Debug, Deserialize)]
struct CheckReply {
    usage: Option<f64>,
    governs: String,
    error: Option<String>,
    #[serde(default)]
    notes: Vec<String>,
}

/// A check over every case.
#[derive(Debug, Deserialize)]
struct SummaryReply {
    check: String,
    usage: Option<f64>,
    case: String,
    governs: String,
    #[serde(default)]
    failed: Vec<(String, String)>,
}

/// One case.
#[derive(Debug, Deserialize)]
struct CaseReply {
    name: String,
    members: Option<CheckReply>,
    pole: Option<CheckReply>,
}

/// The files of a report.
#[derive(Debug, Deserialize)]
struct FilesReply {
    report: PathBuf,
    members_csv: PathBuf,
}

/// The reply (`ohl-structure-reply`, version 1); keys this program does
/// not know are passed over (a newer tool may add some).
#[derive(Debug, Deserialize)]
struct Reply {
    format: String,
    version: u32,
    ok: bool,
    error: Option<String>,
    software: String,
    structure: String,
    extension: Option<String>,
    dz: f64,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    checks: Vec<SummaryReply>,
    #[serde(default)]
    cases: Vec<CaseReply>,
    files: Option<FilesReply>,
}

/// The folders the tool is looked for in: next to `exe_dir` (the running
/// program's folder, and the folder above it: tests run from
/// `target/debug/deps`), then those of `path` (the `PATH`) — as OHL-Design
/// looks for OHL-Structure.
pub fn places(exe_dir: Option<&Path>, path: Option<OsString>) -> Vec<PathBuf> {
    let near = exe_dir
        .into_iter()
        .flat_map(|d| [Some(d.to_path_buf()), d.parent().map(Path::to_path_buf)])
        .flatten();
    let on_path = path
        .iter()
        .flat_map(std::env::split_paths)
        .collect::<Vec<_>>();
    near.chain(on_path).collect()
}

/// The tool in the first of [`places`] that has it.
pub fn find_in(exe_dir: Option<&Path>, path: Option<OsString>) -> Option<PathBuf> {
    let file = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    places(exe_dir, path)
        .into_iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// The tool: the program [`PROGRAM_ENV`] names, else [`find_in`] next to
/// this program or on the `PATH`. Not found: where it was looked for and
/// how to build it.
pub fn find() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os(PROGRAM_ENV).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "{PROGRAM_ENV} names {}, but there is no such file.\n\nBuild the structure \
                 tool (in the OpenOHL folder):\n    {BUILD}\nor unset {PROGRAM_ENV}.",
                p.display()
            ))
        };
    }
    let exe = std::env::current_exe().ok();
    let dir = exe.as_deref().and_then(Path::parent);
    let path = std::env::var_os("PATH");
    find_in(dir, path.clone()).ok_or_else(|| not_found(&places(dir, path)))
}

/// Why the tool was not found: where it was looked for, how to build it.
pub fn not_found(places: &[PathBuf]) -> String {
    let file = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    let list: Vec<String> = places
        .iter()
        .map(|d| format!("    {}", d.display()))
        .collect();
    format!(
        "the structure side ({file}, of OHL-Structure) was not found.\n\nLooked in:\n{}\n\n\
         Build it (in the OpenOHL folder):\n    {BUILD}\nIt is then next to ohl \
         (target/release). Or set {PROGRAM_ENV} to its path.",
        list.join("\n")
    )
}

/// Runs `program --request -` with `request` on its input, `timeout` at
/// most: its output (the reply's text) and its status, or why it did not
/// run (its error output when it stopped without a reply).
fn ask(program: &Path, request: &str, timeout: Duration) -> Result<(String, bool), String> {
    let mut child = Command::new(program)
        .args(["--request", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(request.as_bytes())
            .map_err(|e| format!("{}: {e}", program.display()))?;
    }
    // Both pipes read on threads of their own: neither fills.
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_string(&mut s);
            }
            s
        })
    };
    let out = read(child.stdout.take().map(|p| Box::new(p) as _));
    let err = read(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} did not answer within {} s (stopped; --timeout gives it longer)",
                    program.display(),
                    timeout.as_secs()
                ));
            }
            Err(e) => return Err(format!("{}: {e}", program.display())),
        }
    };
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    if out.trim().is_empty() {
        return Err(format!(
            "{} stopped without a reply ({status}){}",
            program.display(),
            if err.trim().is_empty() {
                String::new()
            } else {
                format!(":\n{}", err.trim())
            }
        ));
    }
    Ok((out, status.success()))
}

fn pct(u: Option<f64>) -> String {
    u.map_or("—".into(), |u| format!("{:.1} %", u * 100.0))
}

/// Asks the tool for `action` and prints the reply.
fn run(args: &StructureRunArgs, action: &str, report: Option<(&Path, bool)>) -> ExitCode {
    let program = match find() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let request = Request {
        format: REQUEST_FORMAT,
        version: PROTOCOL_VERSION,
        action,
        structure: &args.file,
        sections: &args.sections,
        loads: args.loads.as_deref(),
        extension: args.extension.as_deref(),
        cases: &args.cases,
        report: report.map(|r| r.0),
        date: report.is_some_and(|r| r.1),
    };
    let text = serde_json::to_string(&request).expect("plain data");
    let (out, success) = match ask(&program, &text, Duration::from_secs(args.timeout)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if args.json {
        print!("{out}");
        return if success {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let reply: Reply = match serde_json::from_str(&out) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "error: {} gave no reply this program reads ({e}); is it of another build? \
                 Rebuild both:\n    {BUILD}",
                program.display()
            );
            return ExitCode::FAILURE;
        }
    };
    if reply.format != REPLY_FORMAT || reply.version > PROTOCOL_VERSION {
        eprintln!(
            "error: {} answers {} version {} (this program reads {REPLY_FORMAT} up to \
             {PROTOCOL_VERSION}): rebuild both:\n    {BUILD}",
            program.display(),
            reply.format,
            reply.version
        );
        return ExitCode::FAILURE;
    }
    if !reply.ok {
        eprintln!(
            "error: {}",
            reply.error.as_deref().unwrap_or("the structure run failed")
        );
        return ExitCode::FAILURE;
    }
    print_reply(&reply);
    ExitCode::SUCCESS
}

fn print_reply(r: &Reply) {
    println!(
        "{} — {} (by {})",
        r.structure,
        match &r.extension {
            Some(e) => format!("on extension {e} (+{} m)", r.dz),
            None => "as written".into(),
        },
        r.software
    );
    for n in &r.notes {
        println!("  note: {n}");
    }
    for s in &r.checks {
        println!(
            "{}: {} in case {}{}{}",
            s.check,
            pct(s.usage),
            if s.case.is_empty() { "—" } else { &s.case },
            if s.governs.is_empty() {
                String::new()
            } else {
                format!(" ({})", s.governs)
            },
            if s.failed.is_empty() {
                String::new()
            } else {
                format!("; {} case(s) not solved", s.failed.len())
            }
        );
    }
    for c in &r.cases {
        for (what, check) in [("members", &c.members), ("pole", &c.pole)] {
            let Some(k) = check else {
                continue;
            };
            match &k.error {
                Some(e) => println!("  {}: {what}: not solved — {e}", c.name),
                None => println!(
                    "  {}: {what} {}{}",
                    c.name,
                    pct(k.usage),
                    if k.governs.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", k.governs)
                    }
                ),
            }
            for n in &k.notes {
                println!("      {n}");
            }
        }
    }
    if let Some(f) = &r.files {
        println!("→ {}, {}", f.report.display(), f.members_csv.display());
    }
}

/// `ohl structure analyse`.
pub fn analyse(args: &StructureRunArgs) -> ExitCode {
    run(args, "analyse", None)
}

/// `ohl structure report`.
pub fn report(args: &StructureReportArgs) -> ExitCode {
    run(&args.run, "report", Some((&args.output, args.date)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Next to the program (or the folder above it), else on the PATH;
    /// nowhere: where it was looked for and how to build it.
    #[test]
    fn found_next_to_the_program_or_on_the_path() {
        let root = std::env::temp_dir().join(format!("ohl-structure-tool-{}", std::process::id()));
        let (bin, deps, other) = (root.join("bin"), root.join("bin/deps"), root.join("other"));
        for d in [&deps, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        let file = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(find_in(Some(&deps), None), None);
        std::fs::write(other.join(&file), "").unwrap();
        let path = std::env::join_paths([&other]).unwrap();
        assert_eq!(
            find_in(Some(&deps), Some(path.clone())),
            Some(other.join(&file))
        );
        std::fs::write(bin.join(&file), "").unwrap();
        assert_eq!(find_in(Some(&deps), Some(path)), Some(bin.join(&file)));
        let e = not_found(&places(Some(&deps), None));
        assert!(e.contains(BUILD) && e.contains(PROGRAM_ENV), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
