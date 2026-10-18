// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Validation framework.
//!
//! A *validation case* is a set of inputs for one calculation, the reference
//! results expected from it, a tolerance, and the source of the reference
//! (standard, paper, textbook or hand calculation). Cases live as TOML files
//! under `benchmarks/` and are run in CI by `ohl validate`.
//!
//! ```toml
//! [[case]]
//! id = "units-001"
//! title = "Pound-force to newton"
//! calculation = "units.convert"
//! reference = "NIST SP 811 (2008), Appendix B.8"
//!
//! [case.inputs]
//! quantity = "force"
//! value = "1 lbf"
//! to = "N"
//!
//! [case.expected.value]
//! value = 4.448222
//! rel_tol = 1e-6
//! ```
//!
//! Engines expose their calculations to the runner by implementing
//! [`Calculation`] and registering it in a [`Registry`].

mod builtin;

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// Inputs of a validation case, as written in the case file.
pub type Inputs = toml::Table;

/// Named numeric outputs of a calculation.
pub type Outputs = BTreeMap<String, f64>;

/// A calculation that validation cases can exercise.
pub trait Calculation: Send + Sync {
    /// Identifier used in case files, e.g. `"units.convert"`.
    fn id(&self) -> &'static str;

    /// One-line description.
    fn description(&self) -> &'static str;

    /// Runs the calculation. Errors are reported as messages for the case.
    fn run(&self, inputs: &Inputs) -> Result<Outputs, String>;
}

/// The set of calculations available to the runner.
#[derive(Default)]
pub struct Registry {
    calculations: BTreeMap<&'static str, Arc<dyn Calculation>>,
}

impl Registry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry with every calculation shipped with OpenOHL.
    pub fn with_builtin() -> Self {
        let mut registry = Self::new();
        builtin::register(&mut registry);
        registry
    }

    /// Adds a calculation, replacing any calculation with the same id.
    pub fn register(&mut self, calculation: Box<dyn Calculation>) {
        self.calculations
            .insert(calculation.id(), calculation.into());
    }

    /// Looks up a calculation by id.
    pub fn get(&self, id: &str) -> Option<&dyn Calculation> {
        self.calculations.get(id).map(|c| c.as_ref())
    }

    /// All registered calculations, sorted by id.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Calculation> {
        self.calculations.values().map(|c| c.as_ref())
    }
}

/// Expected value of one output, with its tolerance.
///
/// The check passes if the deviation is within `abs_tol` *or* within
/// `rel_tol × |value|`. At least one tolerance must be given.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    /// Reference value.
    pub value: f64,
    /// Absolute tolerance, in the output's unit.
    pub abs_tol: Option<f64>,
    /// Relative tolerance (e.g. `1e-3` for 0.1 %).
    pub rel_tol: Option<f64>,
}

impl Expected {
    /// Whether `actual` is within tolerance of the reference value.
    pub fn accepts(&self, actual: f64) -> bool {
        let deviation = (actual - self.value).abs();
        self.abs_tol.is_some_and(|tol| deviation <= tol)
            || self
                .rel_tol
                .is_some_and(|tol| deviation <= tol * self.value.abs())
    }

    fn tolerance_label(&self) -> String {
        match (self.abs_tol, self.rel_tol) {
            (Some(a), Some(r)) => format!("±{a} or ±{r} rel"),
            (Some(a), None) => format!("±{a}"),
            (None, Some(r)) => format!("±{r} rel"),
            (None, None) => "none".into(),
        }
    }
}

/// One validation case.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Unique identifier, e.g. `"sagtension-012"`.
    pub id: String,
    /// Short title.
    pub title: String,
    /// Id of the [`Calculation`] to run.
    pub calculation: String,
    /// Source of the reference results.
    pub reference: String,
    /// Optional notes (assumptions, units of outputs, …).
    pub notes: Option<String>,
    /// Time limit for the calculation, in seconds (default
    /// [`DEFAULT_CASE_TIME_LIMIT`]). Exceeding it fails the case.
    pub max_seconds: Option<f64>,
    /// Calculation inputs.
    #[serde(default)]
    pub inputs: Inputs,
    /// Expected outputs by name.
    pub expected: BTreeMap<String, Expected>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFile {
    case: Vec<Case>,
}

/// A case together with the file it was loaded from.
#[derive(Debug, Clone)]
pub struct LoadedCase {
    /// Case file.
    pub path: PathBuf,
    /// The case.
    pub case: Case,
}

/// Error while loading case files.
#[derive(Debug)]
pub struct LoadError {
    /// File or directory concerned.
    pub path: PathBuf,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for LoadError {}

/// Parses the cases contained in one TOML document.
pub fn parse_cases(path: &Path, text: &str) -> Result<Vec<LoadedCase>, LoadError> {
    let error = |message: String| LoadError {
        path: path.to_owned(),
        message,
    };
    let file: CaseFile = toml::from_str(text).map_err(|e| error(e.to_string()))?;
    for case in &file.case {
        if case.expected.is_empty() {
            return Err(error(format!("case {}: no expected outputs", case.id)));
        }
        for (name, expected) in &case.expected {
            if expected.abs_tol.is_none() && expected.rel_tol.is_none() {
                return Err(error(format!(
                    "case {}: output {name} has no tolerance",
                    case.id
                )));
            }
        }
    }
    Ok(file
        .case
        .into_iter()
        .map(|case| LoadedCase {
            path: path.to_owned(),
            case,
        })
        .collect())
}

/// Loads every `*.toml` case file under `root` (a file or a directory),
/// in path order. Fails on duplicate case ids.
pub fn load_cases(root: &Path) -> Result<Vec<LoadedCase>, LoadError> {
    let mut files = Vec::new();
    collect_toml_files(root, &mut files)?;
    files.sort();

    let mut cases = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).map_err(|e| LoadError {
            path: path.clone(),
            message: e.to_string(),
        })?;
        cases.extend(parse_cases(&path, &text)?);
    }

    let mut seen = BTreeMap::new();
    for loaded in &cases {
        if let Some(first) = seen.insert(loaded.case.id.as_str(), &loaded.path) {
            return Err(LoadError {
                path: loaded.path.clone(),
                message: format!(
                    "duplicate case id {} (also in {})",
                    loaded.case.id,
                    first.display()
                ),
            });
        }
    }
    Ok(cases)
}

fn collect_toml_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), LoadError> {
    let error = |e: std::io::Error| LoadError {
        path: path.to_owned(),
        message: e.to_string(),
    };
    if path.is_file() {
        files.push(path.to_owned());
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(error)? {
        let entry_path = entry.map_err(error)?.path();
        if entry_path.is_dir() {
            collect_toml_files(&entry_path, files)?;
        } else if entry_path.extension().is_some_and(|ext| ext == "toml") {
            files.push(entry_path);
        }
    }
    Ok(())
}

/// Result of comparing one output with its reference.
#[derive(Debug, Clone)]
pub struct Check {
    /// Output name.
    pub name: String,
    /// Reference value and tolerance.
    pub expected: Expected,
    /// Computed value, if the calculation produced this output.
    pub actual: Option<f64>,
}

impl Check {
    /// Whether the check passed.
    pub fn passed(&self) -> bool {
        self.actual.is_some_and(|a| self.expected.accepts(a))
    }
}

/// Outcome of running one case.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The calculation ran; see the individual checks.
    Ran(Vec<Check>),
    /// The calculation could not run (unknown calculation, bad inputs, …).
    Error(String),
    /// The calculation did not finish within the case's time limit.
    TimedOut(Duration),
}

/// Result of one case.
#[derive(Debug, Clone)]
pub struct CaseResult {
    /// The case that was run.
    pub case: LoadedCase,
    /// What happened.
    pub outcome: Outcome,
    /// Wall-clock time of the calculation (up to the time limit).
    pub elapsed: Duration,
}

impl CaseResult {
    /// Whether every check of the case passed.
    pub fn passed(&self) -> bool {
        match &self.outcome {
            Outcome::Ran(checks) => checks.iter().all(Check::passed),
            Outcome::Error(_) | Outcome::TimedOut(_) => false,
        }
    }
}

/// Time limit of a case that does not set `max_seconds`.
pub const DEFAULT_CASE_TIME_LIMIT: Duration = Duration::from_secs(10);

/// Runs one case, enforcing its time limit.
///
/// The calculation runs on a worker thread. If it exceeds the limit the case
/// is reported as [`Outcome::TimedOut`] and the runner moves on; the worker
/// thread cannot be killed and is left behind, so a process with timed-out
/// cases should exit once the report is written (as `ohl validate` does).
pub fn run_case(registry: &Registry, loaded: LoadedCase) -> CaseResult {
    let case = &loaded.case;
    let limit = case
        .max_seconds
        .map_or(DEFAULT_CASE_TIME_LIMIT, Duration::from_secs_f64);
    let Some(calculation) = registry
        .calculations
        .get(case.calculation.as_str())
        .cloned()
    else {
        let outcome = Outcome::Error(format!("unknown calculation \"{}\"", case.calculation));
        return CaseResult {
            case: loaded,
            outcome,
            elapsed: Duration::ZERO,
        };
    };

    let (sender, receiver) = mpsc::channel();
    let inputs = case.inputs.clone();
    let started = Instant::now();
    let spawned = std::thread::Builder::new()
        .name(format!("validate-{}", case.id))
        .spawn(move || {
            // The receiver is gone if the case already timed out.
            let _ = sender.send(calculation.run(&inputs));
        });
    let outcome = match spawned {
        Err(error) => Outcome::Error(format!("cannot start worker thread: {error}")),
        Ok(_) => match receiver.recv_timeout(limit) {
            Err(mpsc::RecvTimeoutError::Timeout) => Outcome::TimedOut(limit),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Outcome::Error("calculation panicked".into())
            }
            Ok(Err(message)) => Outcome::Error(message),
            Ok(Ok(outputs)) => Outcome::Ran(
                case.expected
                    .iter()
                    .map(|(name, expected)| Check {
                        name: name.clone(),
                        expected: expected.clone(),
                        actual: outputs.get(name).copied(),
                    })
                    .collect(),
            ),
        },
    };
    CaseResult {
        elapsed: started.elapsed().min(limit),
        case: loaded,
        outcome,
    }
}

/// Results of a validation run.
#[derive(Debug, Clone)]
pub struct Report {
    /// One result per case, in run order.
    pub results: Vec<CaseResult>,
}

impl Report {
    /// Runs every case.
    pub fn run(registry: &Registry, cases: Vec<LoadedCase>) -> Self {
        Self {
            results: cases.into_iter().map(|c| run_case(registry, c)).collect(),
        }
    }

    /// Number of passed cases.
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.passed()).count()
    }

    /// Number of failed cases.
    pub fn failed(&self) -> usize {
        self.results.len() - self.passed()
    }

    /// Whether every case passed.
    pub fn all_passed(&self) -> bool {
        self.failed() == 0
    }

    /// Markdown report, suitable for the documentation and CI summaries.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# Validation report\n");
        let _ = writeln!(
            out,
            "**{} cases — {} passed, {} failed.**\n",
            self.results.len(),
            self.passed(),
            self.failed()
        );
        let _ = writeln!(
            out,
            "| Case | Title | Output | Expected | Actual | Tolerance | Time | Result |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
        for result in &self.results {
            let case = &result.case.case;
            let time = format!("{:.1?}", result.elapsed);
            match &result.outcome {
                Outcome::Error(message) => {
                    let _ = writeln!(
                        out,
                        "| {} | {} | — | — | — | — | {} | ❌ error: {} |",
                        case.id, case.title, time, message
                    );
                }
                Outcome::TimedOut(limit) => {
                    let _ = writeln!(
                        out,
                        "| {} | {} | — | — | — | — | {} | ❌ exceeded time limit of {:?} |",
                        case.id, case.title, time, limit
                    );
                }
                Outcome::Ran(checks) => {
                    for check in checks {
                        let actual = check.actual.map_or("missing".into(), |a| format!("{a}"));
                        let mark = if check.passed() { "✅" } else { "❌" };
                        let _ = writeln!(
                            out,
                            "| {} | {} | {} | {} | {} | {} | {} | {} |",
                            case.id,
                            case.title,
                            check.name,
                            check.expected.value,
                            actual,
                            check.expected.tolerance_label(),
                            time,
                            mark
                        );
                    }
                }
            }
        }
        let _ = writeln!(out, "\n## References\n");
        for result in &self.results {
            let case = &result.case.case;
            let _ = writeln!(
                out,
                "- **{}** — {} (`{}`)",
                case.id,
                case.reference,
                result.case.path.display()
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        [[case]]
        id = "t-1"
        title = "Feet to metres"
        calculation = "units.convert"
        reference = "Exact definition"
        inputs = { quantity = "length", value = "1 ft", to = "m" }
        expected.value = { value = 0.3048, abs_tol = 1e-12 }

        [[case]]
        id = "t-2"
        title = "Deliberately wrong"
        calculation = "units.convert"
        reference = "None"
        inputs = { quantity = "length", value = "1 ft", to = "m" }
        expected.value = { value = 0.3, rel_tol = 1e-3 }

        [[case]]
        id = "t-3"
        title = "Unknown calculation"
        calculation = "does.not.exist"
        reference = "None"
        expected.x = { value = 1.0, abs_tol = 0.1 }
    "#;

    #[test]
    fn runs_cases_and_reports() {
        let cases = parse_cases(Path::new("sample.toml"), SAMPLE).unwrap();
        let report = Report::run(&Registry::with_builtin(), cases);
        let passed: Vec<_> = report.results.iter().map(CaseResult::passed).collect();
        assert_eq!(passed, [true, false, false]);
        assert!(!report.all_passed());

        let markdown = report.to_markdown();
        assert!(markdown.contains("3 cases — 1 passed, 2 failed"));
        assert!(markdown.contains("unknown calculation \"does.not.exist\""));
    }

    /// A calculation that never returns.
    struct EndlessLoop;

    impl Calculation for EndlessLoop {
        fn id(&self) -> &'static str {
            "test.endless"
        }

        fn description(&self) -> &'static str {
            "Never returns"
        }

        fn run(&self, _inputs: &Inputs) -> Result<Outputs, String> {
            loop {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn endless_calculation_times_out() {
        let text = r#"
            [[case]]
            id = "t-endless"
            title = "Endless"
            calculation = "test.endless"
            reference = "None"
            max_seconds = 0.2
            expected.x = { value = 1.0, abs_tol = 0.1 }
        "#;
        let mut registry = Registry::new();
        registry.register(Box::new(EndlessLoop));
        let started = Instant::now();
        let report = Report::run(&registry, parse_cases(Path::new("t.toml"), text).unwrap());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(matches!(report.results[0].outcome, Outcome::TimedOut(_)));
        assert!(
            report
                .to_markdown()
                .contains("exceeded time limit of 200ms")
        );
    }

    #[test]
    fn rejects_missing_tolerance() {
        let text = r#"
            [[case]]
            id = "t"
            title = "t"
            calculation = "units.convert"
            reference = "r"
            expected.value = { value = 1.0 }
        "#;
        let error = parse_cases(Path::new("bad.toml"), text).unwrap_err();
        assert!(error.message.contains("no tolerance"), "{error}");
    }

    #[test]
    fn tolerance_rules() {
        let abs = Expected {
            value: 10.0,
            abs_tol: Some(0.5),
            rel_tol: None,
        };
        assert!(abs.accepts(10.4) && !abs.accepts(10.6));
        let rel = Expected {
            value: 10.0,
            abs_tol: None,
            rel_tol: Some(0.01),
        };
        assert!(rel.accepts(10.09) && !rel.accepts(10.2));
    }

    #[test]
    fn repository_benchmarks_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks");
        let cases = load_cases(&root).unwrap();
        assert!(!cases.is_empty());
        let report = Report::run(&Registry::with_builtin(), cases);
        assert!(report.all_passed(), "{}", report.to_markdown());
    }
}
