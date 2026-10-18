// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Python console (Plugins › Python console): an embedded interpreter
//! on its own thread, driving the open project through the `ols` module.
//!
//! Scripts read the line, the weather cases and the solved results, and
//! edit through session commands — the same validated, logged, undoable
//! commands as the tools (ADR-0005): every `ols.apply` is one command,
//! `ols.batch` several as one. A call is answered by the application at
//! its next frame (the model lives there), so a script always sees the
//! line as it is. Output is shown as it comes; Stop (or the time limit)
//! interrupts the script.
//!
//! ```python
//! import ols
//! for s in ols.line()["structures"]:
//!     print(s["id"], s["position"])
//! ols.apply({"op": "move_structure", "id": "T2", "position": [10.0, 0.0, 400.0]})
//! ```

// Without the "python" feature the console only says so.
#![cfg_attr(not(feature = "python"), allow(dead_code, unused_imports))]

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui;

/// What the interpreter sends to the application.
#[derive(Debug)]
pub enum FromPy {
    /// Printed text (`err`: on stderr).
    Output {
        /// The text.
        text: String,
        /// From stderr.
        err: bool,
    },
    /// A call of the `ols` module, answered with [`answer`].
    Call {
        /// Which (`line`, `apply`, …).
        what: String,
        /// Its argument, JSON.
        arg: String,
    },
    /// A run ended (`Err`: it raised).
    Done(Result<(), String>),
}

/// What to run.
#[derive(Debug)]
enum Job {
    /// Console input: in the console's namespace; the value of a last
    /// expression is printed.
    Input(String),
    /// A script file, as `__main__` (its folder first on `sys.path`).
    File(PathBuf),
}

/// The interpreter thread's channels (one interpreter per process).
struct Interpreter {
    jobs: Sender<Job>,
    from: Mutex<Receiver<FromPy>>,
    answers: Sender<Result<String, String>>,
}

static INTERPRETER: OnceLock<Result<Interpreter, String>> = OnceLock::new();

/// The interpreter side of the channels, used by the `ols` functions.
struct Side {
    to_app: Mutex<Sender<FromPy>>,
    answers: Mutex<Receiver<Result<String, String>>>,
}

static SIDE: OnceLock<Side> = OnceLock::new();

/// The console whose script runs (only it takes the interpreter's
/// messages); 0: none.
static OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Consoles made so far (their ids).
static CONSOLES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The Python version embedded, or why there is none.
pub fn version() -> Result<String, String> {
    #[cfg(feature = "python")]
    {
        interpreter()?;
        Ok(pyo3::Python::version_str().to_owned())
    }
    #[cfg(not(feature = "python"))]
    {
        Err("this build has no Python (the \"python\" feature)".into())
    }
}

fn interpreter() -> Result<&'static Interpreter, String> {
    INTERPRETER
        .get_or_init(start)
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(not(feature = "python"))]
fn start() -> Result<Interpreter, String> {
    Err("this build has no Python (the \"python\" feature)".into())
}

#[cfg(feature = "python")]
fn start() -> Result<Interpreter, String> {
    let (jobs, job_rx) = channel::<Job>();
    let (to_app, from) = channel();
    let (answers, answer_rx) = channel();
    SIDE.set(Side {
        to_app: Mutex::new(to_app),
        answers: Mutex::new(answer_rx),
    })
    .map_err(|_| "the interpreter was started twice".to_owned())?;
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    // Interrupts come from a second thread (the worker is busy then).
    let (stop_tx, stop_rx) = channel::<()>();
    std::thread::Builder::new()
        .name("python".into())
        .spawn(move || embedded::run(job_rx, stop_tx, ready_tx))
        .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("python-stop".into())
        .spawn(move || {
            while stop_rx.recv().is_ok() {
                embedded::interrupt();
            }
        })
        .map_err(|e| e.to_string())?;
    // Starting Python takes ~50 ms; 30 s is an exit clause.
    ready_rx
        .recv_timeout(Duration::from_secs(30))
        .map_err(|_| "Python did not start".to_owned())??;
    Ok(Interpreter {
        jobs,
        from: Mutex::new(from),
        answers,
    })
}

#[cfg(feature = "python")]
mod embedded {
    use super::{FromPy, Job, SIDE};
    use pyo3::exceptions::PyRuntimeError;
    use pyo3::prelude::*;
    use pyo3::types::{PyDict, PyModule};
    use std::ffi::CString;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{Receiver, Sender};

    /// The worker's Python thread id (for interrupts); 0 before it runs.
    static THREAD: AtomicU64 = AtomicU64::new(0);

    fn send(m: FromPy) {
        if let Some(s) = SIDE.get()
            && let Ok(tx) = s.to_app.lock()
        {
            let _ = tx.send(m);
        }
    }

    /// Text printed by the script.
    #[pyfunction]
    fn _write(text: &str, err: bool) {
        send(FromPy::Output {
            text: text.to_owned(),
            err,
        });
    }

    /// A call answered by the application (JSON in, JSON out).
    #[pyfunction]
    fn _call(py: Python<'_>, what: &str, arg: &str) -> PyResult<String> {
        send(FromPy::Call {
            what: what.to_owned(),
            arg: arg.to_owned(),
        });
        let side = SIDE
            .get()
            .ok_or_else(|| PyRuntimeError::new_err("no application"))?;
        // Waiting without the interpreter lock: Stop can interrupt.
        let answer = py.detach(|| side.answers.lock().ok().and_then(|rx| rx.recv().ok()));
        match answer {
            Some(Ok(json)) => Ok(json),
            Some(Err(e)) => Err(PyRuntimeError::new_err(e)),
            None => Err(PyRuntimeError::new_err("the application is gone")),
        }
    }

    /// Converts text such as "1000 ft" of a quantity kind to the unit `to`.
    #[pyfunction]
    fn convert(quantity: &str, value: &str, to: &str) -> PyResult<f64> {
        ohl_core::units::convert(quantity, value, to)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Names of the quantity kinds accepted by `convert`.
    #[pyfunction]
    fn quantity_kinds() -> Vec<&'static str> {
        ohl_core::units::QUANTITY_KINDS.to_vec()
    }

    /// The `ols` module's Python part: the calls, the output.
    const BOOT: &str = r#"
import json as _json, sys as _sys

def _j(what, arg=None):
    return _json.loads(_call(what, _json.dumps(arg)))

def line():
    """The working line, as in a line file (a dict)."""
    return _j("line")

def cases():
    """Names of the weather cases."""
    return _j("cases")

def case():
    """Name of the weather case shown."""
    return _j("case")

def results(case=None):
    """Solved sections of a weather case (default: the one shown): per
    section its spans (sag, tensions, low and mid points) and what failed."""
    return _j("results", case)

def selection():
    """The selection, in words."""
    return _j("selection")

def apply(command, what=None):
    """Applies a session command (a dict, e.g. {"op": "move_structure",
    "id": "T2", "position": [x, y, z]}): validated, logged, undoable, the
    sections it touches solved again. Returns what was done."""
    return _j("apply", {"command": command, "what": what})

def batch(commands, what=None):
    """Several commands as one (all or none; one undo)."""
    return apply({"op": "batch", "commands": list(commands)}, what)

def undo():
    """Undoes your last command."""
    return _j("undo")

def redo():
    """Redoes your last undo."""
    return _j("redo")

def status(text):
    """Shows text in the status bar."""
    return _j("status", str(text))

class _Out:
    def __init__(self, err):
        self.err = err
    def write(self, s):
        _write(s, self.err)
        return len(s)
    def flush(self):
        pass
    def isatty(self):
        return False

_sys.stdout = _Out(False)
_sys.stderr = _Out(True)
_sys.stdin = None

def _run(src, name, g):
    import ast
    tree = ast.parse(src, name, "exec")
    last = None
    if tree.body and isinstance(tree.body[-1], ast.Expr):
        last = ast.Expression(tree.body.pop().value)
    exec(compile(tree, name, "exec"), g)
    if last is not None:
        v = eval(compile(last, name, "eval"), g)
        if v is not None:
            print(repr(v))
"#;

    fn module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
        let m = PyModule::new(py, "ols")?;
        m.add("__version__", env!("CARGO_PKG_VERSION"))?;
        m.add_function(wrap_pyfunction!(_write, &m)?)?;
        m.add_function(wrap_pyfunction!(_call, &m)?)?;
        m.add_function(wrap_pyfunction!(convert, &m)?)?;
        m.add_function(wrap_pyfunction!(quantity_kinds, &m)?)?;
        py.import("sys")?.getattr("modules")?.set_item("ols", &m)?;
        let code = CString::new(BOOT).expect("no NUL");
        py.run(&code, Some(&m.dict()), None)?;
        Ok(m)
    }

    fn error_text(py: Python<'_>, e: &PyErr) -> String {
        // The traceback goes to the console (stderr); the summary is kept.
        e.print(py);
        e.to_string()
    }

    pub(super) fn run(jobs: Receiver<Job>, stop: Sender<()>, ready: Sender<Result<(), String>>) {
        let started = Python::attach(|py| -> PyResult<Py<PyDict>> {
            let m = module(py)?;
            let ident: u64 = py
                .import("threading")?
                .call_method0("get_ident")?
                .extract()?;
            THREAD.store(ident, Ordering::SeqCst);
            let globals = PyDict::new(py);
            globals.set_item("__name__", "__console__")?;
            globals.set_item("__builtins__", py.import("builtins")?)?;
            globals.set_item("ols", m)?;
            Ok(globals.unbind())
        });
        let globals = match started {
            Ok(g) => {
                let _ = ready.send(Ok(()));
                g
            }
            Err(e) => {
                let _ = ready.send(Err(e.to_string()));
                return;
            }
        };
        while let Ok(job) = jobs.recv() {
            let result = Python::attach(|py| -> Result<(), String> {
                let m = py.import("ols").map_err(|e| e.to_string())?;
                let run = m.getattr("_run").map_err(|e| e.to_string())?;
                let r = match job {
                    Job::Input(code) => run.call1((code, "<console>", globals.bind(py))),
                    Job::File(path) => {
                        let text = std::fs::read_to_string(&path)
                            .map_err(|e| format!("{}: {e}", path.display()))?;
                        let g = PyDict::new(py);
                        let setup = (|| -> PyResult<()> {
                            g.set_item("__name__", "__main__")?;
                            g.set_item("__file__", path.display().to_string())?;
                            g.set_item("__builtins__", py.import("builtins")?)?;
                            if let Some(dir) = path.parent() {
                                py.import("sys")?
                                    .getattr("path")?
                                    .call_method1("insert", (0, dir.display().to_string()))?;
                            }
                            Ok(())
                        })();
                        setup.map_err(|e| e.to_string())?;
                        run.call1((text, path.display().to_string(), g))
                    }
                };
                r.map(|_| ()).map_err(|e| error_text(py, &e))
            });
            send(FromPy::Done(result));
        }
        drop(stop);
    }

    /// Raises `KeyboardInterrupt` in the worker thread (at its next Python
    /// instruction; a call waiting for the application first gets its
    /// answer).
    pub(super) fn interrupt() {
        let ident = THREAD.load(Ordering::SeqCst);
        if ident == 0 {
            return;
        }
        Python::attach(|py| {
            let code = format!(
                "import ctypes\nctypes.pythonapi.PyThreadState_SetAsyncExc(ctypes.c_ulong({ident}), \
                 ctypes.py_object(KeyboardInterrupt))"
            );
            if let Ok(c) = CString::new(code) {
                let _ = py.run(&c, None, None);
            }
        });
    }

    #[cfg(test)]
    pub(super) fn thread_ident() -> u64 {
        THREAD.load(Ordering::SeqCst)
    }
}

/// Answers the interpreter's call (`Err`: raised in the script).
pub fn answer(result: Result<String, String>) {
    if let Ok(i) = interpreter() {
        let _ = i.answers.send(result);
    }
}

/// Answers a call of the `ols` module from the open project (`arg`:
/// JSON): the line, the weather cases, results, the selection; commands
/// applied like the tools' (one each, undoable).
pub fn serve(r: &mut crate::app::Ready, what: &str, arg: &str) -> Result<String, String> {
    use serde_json::{Value, json};
    let arg: Value = serde_json::from_str(arg).map_err(|e| e.to_string())?;
    let out = match what {
        "line" => serde_json::to_value(&r.model.line).map_err(|e| e.to_string())?,
        "cases" => json!(
            r.model
                .cases
                .iter()
                .map(|c| c.case.name.clone())
                .collect::<Vec<_>>()
        ),
        "case" => json!(r.model.cases.get(r.case).map(|c| c.case.name.clone())),
        "selection" => json!(r.selection.describe()),
        "results" => {
            let k = match arg.as_str() {
                Some(name) => r
                    .model
                    .cases
                    .iter()
                    .position(|c| c.case.name == name)
                    .ok_or_else(|| format!("no weather case {name}"))?,
                None => r.case,
            };
            // Solved now if it is not yet.
            r.model.ensure_solved(k);
            let solved = r.model.cases.get(k).ok_or("no weather case")?;
            json!({
                "case": solved.case.name,
                "sections": solved.shapes.iter().map(|s| json!({
                    "section": s.section,
                    "spans": s.spans.iter().map(|p| json!({
                        "sag": p.sag,
                        "horizontal_tension": p.horizontal_tension,
                        "max_tension": p.max_tension,
                        "mid_point": p.mid_point,
                        "low_point": p.low_point,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "failed": solved.failed,
            })
        }
        "apply" | "undo" | "redo" => {
            r.model.can_edit().map_err(|e| format!("read-only: {e}"))?;
            let action = match what {
                "undo" => crate::edit::Action::Undo,
                "redo" => crate::edit::Action::Redo,
                _ => {
                    let command: ohl_session::Command =
                        serde_json::from_value(arg["command"].clone())
                            .map_err(|e| format!("not a command: {e}"))?;
                    let what = arg["what"]
                        .as_str()
                        .map_or_else(|| "python script".to_owned(), str::to_owned);
                    crate::edit::Action::Command { what, command }
                }
            };
            let done = r.perform(action);
            if r.confirm.take().is_some() {
                return Err("this takes other objects with it: add \"confirm\": true".into());
            }
            let text = r.status.as_ref().map_or(String::new(), |s| s.0.clone());
            if !done {
                return Err(text);
            }
            json!(text)
        }
        "status" => {
            r.status = Some((arg.as_str().unwrap_or("").to_owned(), false));
            Value::Null
        }
        _ => return Err(format!("unknown call {what}")),
    };
    Ok(out.to_string())
}

/// One line of the console's output.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// What was typed.
    Input(String),
    /// Printed.
    Out(String),
    /// Printed on stderr, or an error.
    Err(String),
}

/// The console page.
pub struct Console {
    /// The output (input echoed).
    pub lines: Vec<Line>,
    /// What is being typed.
    pub input: String,
    /// Inputs given, oldest first.
    pub history: Vec<String>,
    /// Where in the history (Up / Down), from the end.
    back: usize,
    /// A script running since.
    pub running: Option<Instant>,
    /// Longest a script may run, s (an exit clause; Stop also works).
    pub limit: f64,
    /// Stop asked (not yet stopped).
    stopping: bool,
    /// This console's id ([`OWNER`]).
    id: u64,
}

impl Default for Console {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            input: String::new(),
            history: Vec::new(),
            back: 0,
            running: None,
            limit: 300.0,
            stopping: false,
            id: CONSOLES.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1,
        }
    }
}

/// How many output lines are kept.
const MAX_LINES: usize = 5000;

impl Console {
    fn push(&mut self, line: Line) {
        if matches!(&line, Line::Out(t) | Line::Err(t) if t.is_empty()) {
            return;
        }
        // Printed text comes in pieces: join them into lines.
        match (&line, self.lines.last_mut()) {
            (Line::Out(t), Some(Line::Out(last))) | (Line::Err(t), Some(Line::Err(last)))
                if !last.ends_with('\n') =>
            {
                last.push_str(t);
            }
            _ => self.lines.push(line),
        }
        if self.lines.len() > MAX_LINES {
            self.lines.drain(..self.lines.len() - MAX_LINES);
        }
    }

    /// Runs console input.
    pub fn submit(&mut self, code: String) {
        if code.trim().is_empty() || self.running.is_some() {
            return;
        }
        self.push(Line::Input(code.clone()));
        if self.history.last() != Some(&code) {
            self.history.push(code.clone());
        }
        self.back = 0;
        self.send(Job::Input(code));
    }

    /// Runs a script file.
    pub fn run_file(&mut self, path: PathBuf) {
        if self.running.is_some() {
            return;
        }
        self.push(Line::Input(format!("# run {}", path.display())));
        self.send(Job::File(path));
    }

    fn send(&mut self, job: Job) {
        use std::sync::atomic::Ordering;
        match interpreter() {
            Ok(i) => {
                // One script at a time, whichever console runs it.
                if OWNER
                    .compare_exchange(0, self.id, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    self.push(Line::Err("another script is running\n".into()));
                    return;
                }
                if i.jobs.send(job).is_ok() {
                    self.running = Some(Instant::now());
                    self.stopping = false;
                } else {
                    OWNER.store(0, Ordering::SeqCst);
                }
            }
            Err(e) => self.push(Line::Err(format!("{e}\n"))),
        }
    }

    /// Interrupts the running script.
    pub fn stop(&mut self) {
        if self.running.is_none() || self.stopping {
            return;
        }
        self.stopping = true;
        #[cfg(feature = "python")]
        std::thread::spawn(embedded::interrupt);
    }

    /// Takes what the interpreter sent: output, the end of a run, and the
    /// calls to answer (returned, in order). Stops a script past the time
    /// limit.
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<(String, String)> {
        let mut calls = Vec::new();
        let Some(Ok(i)) = INTERPRETER.get() else {
            return calls;
        };
        if OWNER.load(std::sync::atomic::Ordering::SeqCst) != self.id {
            return calls;
        }
        if let Ok(rx) = i.from.lock() {
            while let Ok(m) = rx.try_recv() {
                match m {
                    FromPy::Output { text, err } => {
                        self.push(if err {
                            Line::Err(text)
                        } else {
                            Line::Out(text)
                        });
                    }
                    FromPy::Call { what, arg } => calls.push((what, arg)),
                    FromPy::Done(r) => {
                        OWNER.store(0, std::sync::atomic::Ordering::SeqCst);
                        self.running = None;
                        self.stopping = false;
                        if let Err(e) = r
                            && !matches!(self.lines.last(), Some(Line::Err(_)))
                        {
                            self.push(Line::Err(format!("{e}\n")));
                        }
                    }
                }
            }
        }
        if let Some(t) = self.running {
            if t.elapsed().as_secs_f64() > self.limit && !self.stopping {
                self.push(Line::Err(format!(
                    "stopped: longer than the time limit ({:.0} s)\n",
                    self.limit
                )));
                self.stop();
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        calls
    }

    /// The page: output, input, Run file / Stop / Clear, the scripts of
    /// the Python plugins (`scripts`). Returns a script to run.
    pub fn ui(&mut self, ui: &mut egui::Ui, scripts: &[(String, PathBuf)]) -> Option<PathBuf> {
        let mut run: Option<PathBuf> = None;
        ui.horizontal(|ui| {
            match version() {
                Ok(v) => ui.label(
                    egui::RichText::new(format!(
                        "Python {} — import ols: line(), cases(), results(), apply(), batch(), \
                         undo()",
                        v.split_whitespace().next().unwrap_or("")
                    ))
                    .small()
                    .weak(),
                ),
                Err(e) => crate::menu::error_label(ui, e),
            };
        });
        ui.horizontal(|ui| {
            let idle = self.running.is_none();
            if ui
                .add_enabled(idle, egui::Button::new("Run a file…"))
                .on_hover_text("Run a Python script (as __main__)")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Python", &["py"])
                    .pick_file()
            {
                run = Some(p);
            }
            ui.add_enabled_ui(idle && !scripts.is_empty(), |ui| {
                ui.menu_button("Plugin scripts", |ui| {
                    for (name, path) in scripts {
                        if ui
                            .button(name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            run = Some(path.clone());
                            ui.close();
                        }
                    }
                });
            });
            if ui
                .add_enabled(!idle, egui::Button::new("Stop"))
                .on_hover_text("Interrupt the script (KeyboardInterrupt)")
                .clicked()
            {
                self.stop();
            }
            if ui.button("Clear").clicked() {
                self.lines.clear();
            }
            ui.label("time limit");
            ui.add(
                egui::DragValue::new(&mut self.limit)
                    .range(1.0..=86_400.0)
                    .suffix(" s"),
            )
            .on_hover_text("A script running longer is stopped");
            if let Some(t) = self.running {
                ui.spinner();
                ui.label(format!("running {:.1} s", t.elapsed().as_secs_f64()));
            }
        });
        ui.separator();
        let input_height = 70.0;
        egui::ScrollArea::vertical()
            .max_height((ui.available_height() - input_height).max(60.0))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for l in &self.lines {
                    let (text, color) = match l {
                        Line::Input(t) => (format!(">>> {}", t.replace('\n', "\n... ")), None),
                        Line::Out(t) => (t.trim_end_matches('\n').to_owned(), None),
                        Line::Err(t) => (
                            t.trim_end_matches('\n').to_owned(),
                            Some(ui.visuals().error_fg_color),
                        ),
                    };
                    let mut rt = egui::RichText::new(text).monospace();
                    if let Some(c) = color {
                        rt = rt.color(c);
                    }
                    if matches!(l, Line::Input(_)) {
                        rt = rt.weak();
                    }
                    ui.add(egui::Label::new(rt).wrap_mode(egui::TextWrapMode::Extend));
                }
            });
        ui.separator();
        let r = ui.add(
            egui::TextEdit::multiline(&mut self.input)
                .id_salt("python-input")
                .code_editor()
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .hint_text("Python — Enter runs, Shift+Enter a new line, ⬆ ⬇ history"),
        );
        if r.has_focus() {
            let (enter, up, down) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter) && !i.modifiers.shift,
                    i.key_pressed(egui::Key::ArrowUp) && !self.input.contains('\n'),
                    i.key_pressed(egui::Key::ArrowDown) && !self.input.contains('\n'),
                )
            });
            if enter {
                // The newline Enter typed is not part of the code.
                let code = self.input.trim_end_matches('\n').to_owned();
                self.input.clear();
                self.submit(code);
            } else if up && self.back < self.history.len() {
                self.back += 1;
                self.input = self.history[self.history.len() - self.back].clone();
            } else if down && self.back > 0 {
                self.back -= 1;
                self.input = if self.back == 0 {
                    String::new()
                } else {
                    self.history[self.history.len() - self.back].clone()
                };
            }
        }
        run
    }
}

/// Tests using the interpreter (one per process) take turns.
#[cfg(test)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(all(test, feature = "python"))]
mod tests {
    use super::*;

    /// Runs, prints, raises, is interrupted; calls reach the application.
    /// (One test: the interpreter is shared by the process.)
    #[test]
    fn the_interpreter_runs_prints_calls_and_stops() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut c = Console::default();
        let wait = |c: &mut Console, calls: &mut Vec<(String, String)>| {
            let t = Instant::now();
            while c.running.is_some() {
                assert!(t.elapsed() < Duration::from_secs(20), "still running");
                for call in c.poll(&ctx) {
                    answer(Ok(format!("{{\"echo\": {}}}", call.1)));
                    calls.push(call);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let mut calls = Vec::new();
        c.submit("x = 6 * 7\nx".into());
        wait(&mut c, &mut calls);
        assert_eq!(c.lines.last(), Some(&Line::Out("42\n".into())));
        assert!(embedded::thread_ident() != 0);
        // The namespace persists; ols is there.
        c.submit("print(ols.convert('length', '1000 ft', 'm'), x)".into());
        wait(&mut c, &mut calls);
        assert_eq!(c.lines.last(), Some(&Line::Out("304.8 42\n".into())));
        // A call goes to the application and comes back.
        c.submit("ols.results('hot')".into());
        wait(&mut c, &mut calls);
        assert_eq!(calls.last().unwrap().0, "results");
        assert_eq!(c.lines.last(), Some(&Line::Out("{'echo': 'hot'}\n".into())));
        // An error: its traceback.
        c.submit("1 / 0".into());
        wait(&mut c, &mut calls);
        let text: String = c
            .lines
            .iter()
            .filter_map(|l| match l {
                Line::Err(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains("ZeroDivisionError"), "{text}");
        // An endless loop is stopped by the time limit.
        c.limit = 0.3;
        let t = Instant::now();
        c.submit("while True:\n    pass".into());
        wait(&mut c, &mut calls);
        assert!(t.elapsed() < Duration::from_secs(5));
        assert!(
            matches!(c.lines.last(), Some(Line::Err(t)) if t.contains("KeyboardInterrupt")),
            "{:?}",
            c.lines.last()
        );
        // Still usable.
        c.limit = 300.0;
        c.submit("x + 1".into());
        wait(&mut c, &mut calls);
        assert_eq!(c.lines.last(), Some(&Line::Out("43\n".into())));
    }
}
