// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl project commit|history|status|branch|checkout|tag|diff|merge|revert|
//! review|grant|bundle`: version control and collaboration of a project
//! file (`ohl_session::versions`, `ohl_session::collab`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Subcommand;
use ohl_session::collab::Grant;
use ohl_session::versions::{
    Change, Choices, Diff, Head, MergeKind, MergeOutcome, RefKind, Side, State, diff, short,
};
use ohl_session::{Session, SessionError};

#[derive(Subcommand)]
pub enum VersionCommand {
    /// Record the working state (line and libraries) as a revision on the
    /// current branch.
    Commit {
        /// Project file.
        project: PathBuf,
        /// User name.
        #[arg(long)]
        user: String,
        /// Why: "Raised T12 by 3 m for the road crossing".
        #[arg(long, short)]
        message: String,
    },
    /// The revisions, newest first, with their branches and tags (the
    /// operation log is `ohl project log`).
    History {
        /// Project file.
        project: PathBuf,
        /// At most this many.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// The current branch and the uncommitted changes.
    Status {
        /// Project file.
        project: PathBuf,
    },
    /// List branches and tags; with NAME make a branch (or protect,
    /// unprotect or delete one).
    Branch {
        /// Project file.
        project: PathBuf,
        /// Branch name.
        name: Option<String>,
        /// User name.
        #[arg(long)]
        user: Option<String>,
        /// Make it at this branch, tag or revision (default: the head).
        #[arg(long)]
        at: Option<String>,
        /// Admin: protect it (changed only by admins or approved reviews).
        #[arg(long, conflicts_with_all = ["unprotect", "delete"])]
        protect: bool,
        /// Admin: lift the protection.
        #[arg(long, conflicts_with = "delete")]
        unprotect: bool,
        /// Delete it (lists what goes with it; applied with --confirm).
        #[arg(long)]
        delete: bool,
        /// Confirms the deletion.
        #[arg(long)]
        confirm: bool,
    },
    /// Work on project line (branch) TARGET: the commands act on it from
    /// now on. Nothing changes; uncommitted work stays on its line. A tag
    /// or revision is worked on as a new line: branch NAME --at TAG.
    Checkout {
        /// Project file.
        project: PathBuf,
        /// Branch, tag or revision.
        target: String,
        /// User name.
        #[arg(long)]
        user: String,
    },
    /// Name a revision for good, e.g. "IFC rev B" (or delete a tag).
    Tag {
        /// Project file.
        project: PathBuf,
        /// Tag name.
        name: String,
        /// User name.
        #[arg(long)]
        user: String,
        /// The revision (default: the head).
        #[arg(long)]
        at: Option<String>,
        /// Note.
        #[arg(long, default_value = "")]
        note: String,
        /// Admin: delete it (with --confirm).
        #[arg(long)]
        delete: bool,
        /// Confirms the deletion.
        #[arg(long)]
        confirm: bool,
    },
    /// Differences between two revisions (default: the head and the
    /// working state), field by field; --impact solves the touched
    /// sections on both sides.
    Diff {
        /// Project file.
        project: PathBuf,
        /// From: branch, tag or revision (default: HEAD).
        from: Option<String>,
        /// To (default: the working state).
        to: Option<String>,
        /// Only the objects, without their fields.
        #[arg(long)]
        brief: bool,
        /// Solve the touched sections on both sides: largest sag and
        /// tension, smallest ground clearance.
        #[arg(long)]
        impact: bool,
        /// Weather case file for --impact (default: still air at
        /// --temperature).
        #[arg(long)]
        weather: Option<PathBuf>,
        /// Case of the weather file (default: its first).
        #[arg(long)]
        case: Option<String>,
        /// Cable temperature without a weather file, °C.
        #[arg(long, default_value_t = 15.0)]
        temperature: f64,
        /// Wires for states without a wire library (default: the shipped
        /// libraries/cables).
        #[arg(long)]
        library: Option<PathBuf>,
        /// Ground surface of the project for clearances (default: the
        /// surface of the checks).
        #[arg(long)]
        surface: Option<String>,
        /// Sections solved at most.
        #[arg(long, default_value_t = 500)]
        max_sections: usize,
        /// Time limit, s.
        #[arg(long, default_value_t = 60.0)]
        time_limit: f64,
    },
    /// Merge a branch (tag, revision) into the current branch; conflicts
    /// are listed and settled with --choose KEY=ours|theirs.
    Merge {
        /// Project file.
        project: PathBuf,
        /// Branch, tag or revision.
        other: String,
        /// User name.
        #[arg(long)]
        user: String,
        /// Message of the merge revision.
        #[arg(long, short)]
        message: Option<String>,
        /// KEY=ours|theirs (repeatable); KEY as listed (`structure:T6`,
        /// `structure:T6/position`, `library:cables/x.ohlw`).
        #[arg(long = "choose")]
        choose: Vec<String>,
        /// Only show what the merge would do.
        #[arg(long)]
        preview: bool,
    },
    /// A new revision undoing another's changes.
    Revert {
        /// Project file.
        project: PathBuf,
        /// The revision.
        revision: String,
        /// User name.
        #[arg(long)]
        user: String,
        /// Message.
        #[arg(long, short)]
        message: Option<String>,
        /// KEY=ours|theirs (repeatable), for changes made since.
        #[arg(long = "choose")]
        choose: Vec<String>,
    },
    /// Reviews: list; --submit BRANCH; --approve/--reject/--merge ID.
    Review {
        /// Project file.
        project: PathBuf,
        /// User name.
        #[arg(long)]
        user: Option<String>,
        /// Submit this branch for review.
        #[arg(long, conflicts_with_all = ["approve", "reject", "merge"])]
        submit: Option<String>,
        /// Branch to merge it into.
        #[arg(long, default_value = "main")]
        into: String,
        /// Reviewer (repeatable).
        #[arg(long = "reviewer")]
        reviewers: Vec<String>,
        /// Approve review ID.
        #[arg(long, conflicts_with_all = ["reject", "merge"])]
        approve: Option<i64>,
        /// Reject review ID.
        #[arg(long, conflicts_with = "merge")]
        reject: Option<i64>,
        /// Merge approved review ID into its target project line.
        #[arg(long)]
        merge: Option<i64>,
        /// Note or comment.
        #[arg(long, default_value = "")]
        note: String,
        /// KEY=ours|theirs for the merge (repeatable).
        #[arg(long = "choose")]
        choose: Vec<String>,
    },
    /// Admin: limit what a user (or role:NAME) may edit; without
    /// --subject lists the grants.
    Grant {
        /// Project file.
        project: PathBuf,
        /// Admin doing it.
        #[arg(long)]
        by: Option<String>,
        /// User, or role:designer.
        #[arg(long)]
        subject: Option<String>,
        /// Kind (repeatable): structure, section, jumper, route, meta,
        /// library.
        #[arg(long = "kind")]
        kinds: Vec<String>,
        /// Object (repeatable): T12 or structure:T12.
        #[arg(long = "object")]
        objects: Vec<String>,
        /// Plan area "X,Y;X,Y;X,Y…" (m, project coordinates).
        #[arg(long, allow_hyphen_values = true)]
        area: Option<String>,
        /// Note.
        #[arg(long, default_value = "")]
        note: String,
        /// Remove grant ID.
        #[arg(long, conflicts_with = "subject")]
        revoke: Option<i64>,
    },
    /// Offline work: write a branch with its history to a file, or read
    /// one back as a branch (then merge it).
    Bundle {
        /// Project file.
        project: PathBuf,
        /// Branch to write.
        #[arg(long, requires = "out", conflicts_with = "import")]
        export: Option<String>,
        /// Bundle file written.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Bundle file read.
        #[arg(long, requires = "user")]
        import: Option<PathBuf>,
        /// Branch name for the import (default: the bundle's).
        #[arg(long = "as")]
        as_branch: Option<String>,
        /// User name.
        #[arg(long)]
        user: Option<String>,
    },
}

fn open(path: &Path) -> Result<Session, String> {
    Session::open(path).map_err(|e| e.to_string())
}

fn err(e: SessionError) -> String {
    e.to_string()
}

fn need<'a>(user: &'a Option<String>, what: &str) -> Result<&'a str, String> {
    user.as_deref()
        .ok_or_else(|| format!("{what}: give --user (or --by)"))
}

fn choices(list: &[String]) -> Result<Choices, String> {
    list.iter()
        .map(|c| {
            let (k, side) = c
                .rsplit_once('=')
                .ok_or_else(|| format!("--choose \"{c}\": KEY=ours or KEY=theirs"))?;
            Ok((k.trim().to_owned(), Side::parse(side.trim()).map_err(err)?))
        })
        .collect()
}

fn value(v: &Option<serde_json::Value>) -> String {
    match v {
        None => "(none)".into(),
        Some(v) => {
            let t = v.to_string();
            if t.chars().count() > 70 {
                format!("{}…", t.chars().take(70).collect::<String>())
            } else {
                t
            }
        }
    }
}

fn print_diff(d: &Diff, brief: bool) {
    println!("{}", d.summary());
    for o in &d.objects {
        let sign = match o.change {
            Change::Added => '+',
            Change::Removed => '-',
            Change::Changed => '~',
        };
        println!("  {sign} {} {}", o.kind.as_str(), o.id);
        if !brief {
            for f in &o.fields {
                println!(
                    "      {}: {} → {}",
                    f.path,
                    value(&f.before),
                    value(&f.after)
                );
            }
        }
    }
    for l in &d.libraries {
        println!(
            "  {} library {}/{} ({} lines out, {} in)",
            match l.change {
                Change::Added => '+',
                Change::Removed => '-',
                Change::Changed => '~',
            },
            l.kind,
            l.name,
            l.lines.0,
            l.lines.1
        );
    }
}

fn outcome(what: &str, o: &MergeOutcome) {
    match (&o.kind, &o.revision) {
        (MergeKind::FastForward, _) => println!("{what}: fast-forward (no merge revision)"),
        (_, Some(r)) => println!("{what}: revision {} \"{}\"", r.short(), r.message),
        _ => println!("{what}"),
    }
    if let Some(a) = &o.applied {
        println!(
            "  working state: log entry {}, {} object(s), {} section(s) to recalculate",
            a.seq,
            a.objects.len(),
            a.sections.len()
        );
    }
    if o.libraries_changed {
        println!("  library files changed");
    }
}

fn state_of(s: &Session, name: Option<&str>) -> Result<(String, State), String> {
    match name {
        None => Ok(("working state".into(), s.working_state().map_err(err)?)),
        Some(n) => {
            let id = s.resolve_revision(n).map_err(err)?;
            Ok((
                format!("{n} ({})", short(&id)),
                s.revision_state(&id).map_err(err)?,
            ))
        }
    }
}

pub fn run(command: VersionCommand) -> Result<(), String> {
    match command {
        VersionCommand::Commit {
            project,
            user,
            message,
        } => {
            let mut s = open(&project)?;
            let r = s.commit(&user, &message).map_err(err)?;
            let branch = match s.head().map_err(err)? {
                Head::Branch(b) => b,
                Head::Detached(_) => String::new(),
            };
            println!("[{branch} {}] {}", r.short(), r.message);
        }
        VersionCommand::History { project, limit } => {
            let s = open(&project)?;
            let refs = s.refs().map_err(err)?;
            let head = s.head_revision().map_err(err)?;
            let revs = s.revisions().map_err(err)?;
            if revs.is_empty() {
                println!("no revision yet: ohl project commit");
            }
            for r in revs.iter().take(limit) {
                let mut labels: Vec<String> = refs
                    .iter()
                    .filter(|x| x.revision.as_deref() == Some(r.id.as_str()))
                    .map(|x| match x.kind {
                        RefKind::Branch => x.name.clone(),
                        RefKind::Tag => format!("tag: {}", x.name),
                    })
                    .collect();
                if head.as_deref() == Some(r.id.as_str()) {
                    labels.insert(0, "HEAD".into());
                }
                println!(
                    "{} {}{}",
                    if r.parents.len() > 1 { "M" } else { "*" },
                    r.short(),
                    if labels.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", labels.join(", "))
                    }
                );
                println!(
                    "    {}  {}  after log entry {}{}",
                    r.time.get(..19).unwrap_or(&r.time),
                    r.author,
                    r.seq,
                    if r.parents.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "  parents {}",
                            r.parents
                                .iter()
                                .map(|p| short(p))
                                .collect::<Vec<_>>()
                                .join(" ")
                        )
                    }
                );
                println!("    {}", r.message);
            }
            if revs.len() > limit {
                println!("… {} older", revs.len() - limit);
            }
        }
        VersionCommand::Status { project } => {
            let s = open(&project)?;
            let st = s.status().map_err(err)?;
            match &st.head {
                Head::Branch(b) => println!(
                    "on branch {b}{}",
                    st.revision
                        .as_deref()
                        .map(|r| format!(" at {}", short(r)))
                        .unwrap_or_else(|| " (no revision yet)".into())
                ),
                Head::Detached(r) => println!("revision {} checked out (detached)", short(r)),
            }
            if st.changes.is_empty() {
                println!("nothing to commit");
            } else {
                print!("uncommitted: ");
                print_diff(&st.changes, true);
            }
        }
        VersionCommand::Branch {
            project,
            name,
            user,
            at,
            protect,
            unprotect,
            delete,
            confirm,
        } => {
            let mut s = open(&project)?;
            if let Some(name) = name {
                let user = need(&user, "branch")?;
                if delete {
                    match s.delete_ref(user, &name, confirm) {
                        Ok(d) => println!("{d}: done"),
                        Err(e) => {
                            if !confirm {
                                println!("{e}");
                                println!("Nothing was changed. Repeat with --confirm.");
                                return Err("confirmation needed".into());
                            }
                            return Err(e.to_string());
                        }
                    }
                } else if protect || unprotect {
                    s.protect_branch(user, &name, protect).map_err(err)?;
                    println!(
                        "branch {name} {}",
                        if protect { "protected" } else { "unprotected" }
                    );
                } else {
                    let r = s.create_branch(user, &name, at.as_deref()).map_err(err)?;
                    println!(
                        "project line {name} made at {} (ohl project checkout to work on it)",
                        r.as_deref().map_or("its source's working state", short)
                    );
                }
                return Ok(());
            }
            let head = s.head().map_err(err)?;
            for r in s.refs().map_err(err)? {
                let current = head == Head::Branch(r.name.clone());
                println!(
                    "{} {:<6} {:<24} {}{}{}",
                    if current { "*" } else { " " },
                    match r.kind {
                        RefKind::Branch => "branch",
                        RefKind::Tag => "tag",
                    },
                    r.name,
                    r.revision.as_deref().map(short).unwrap_or("(no revision)"),
                    if r.protected { "  protected" } else { "" },
                    if r.note.is_empty() {
                        String::new()
                    } else {
                        format!("  {}", r.note)
                    }
                );
            }
        }
        VersionCommand::Checkout {
            project,
            target,
            user,
        } => {
            let mut s = open(&project)?;
            s.checkout(&user, &target).map_err(err)?;
            println!("working on project line {target} (the commands act on it; nothing changed)");
        }
        VersionCommand::Tag {
            project,
            name,
            user,
            at,
            note,
            delete,
            confirm,
        } => {
            let mut s = open(&project)?;
            if delete {
                match s.delete_ref(&user, &name, confirm) {
                    Ok(d) => println!("{d}: done"),
                    Err(e) if !confirm => {
                        println!("{e}");
                        println!("Nothing was changed. Repeat with --confirm.");
                        return Err("confirmation needed".into());
                    }
                    Err(e) => return Err(e.to_string()),
                }
            } else {
                let r = s.tag(&user, &name, at.as_deref(), &note).map_err(err)?;
                println!("tag \"{name}\" at {}", short(&r));
            }
        }
        VersionCommand::Diff {
            project,
            from,
            to,
            brief,
            impact,
            weather,
            case,
            temperature,
            library,
            surface,
            max_sections,
            time_limit,
        } => {
            let s = open(&project)?;
            let from = from.unwrap_or_else(|| "HEAD".into());
            let (na, a) = state_of(&s, Some(&from))?;
            let (nb, b) = state_of(&s, to.as_deref())?;
            println!("{na} → {nb}");
            let d = diff(&a, &b);
            print_diff(&d, brief);
            if impact && !d.is_empty() {
                print_impact(
                    &s,
                    &a,
                    &b,
                    &d,
                    ImpactArgs {
                        weather,
                        case,
                        temperature,
                        library,
                        surface,
                        max_sections,
                        time_limit,
                    },
                )?;
            }
        }
        VersionCommand::Merge {
            project,
            other,
            user,
            message,
            choose,
            preview,
        } => {
            let mut s = open(&project)?;
            let choices = choices(&choose)?;
            let plan = s.merge_plan(&other, &choices).map_err(err)?;
            let kind = match plan.kind {
                MergeKind::UpToDate => "already contained",
                MergeKind::FastForward => "fast-forward",
                MergeKind::ThreeWay => "three-way merge",
            };
            println!(
                "{other} ({}): {kind}{}",
                short(&plan.theirs),
                plan.base
                    .as_deref()
                    .map(|b| format!(", common ancestor {}", short(b)))
                    .unwrap_or_default()
            );
            if !plan.changes.is_empty() {
                print!("changes to the working state: ");
                print_diff(&plan.changes, true);
            }
            for c in &plan.conflicts {
                println!(
                    "  conflict {}: base {}, ours {}, theirs {}{}",
                    c.key,
                    value(&c.base),
                    value(&c.ours),
                    value(&c.theirs),
                    match c.chosen {
                        Some(Side::Ours) => " — ours chosen",
                        Some(Side::Theirs) => " — theirs chosen",
                        None => "",
                    }
                );
            }
            if let Some(p) = &plan.problem {
                println!("the merged line is not valid: {p}");
            }
            if preview || plan.kind == MergeKind::UpToDate {
                return Ok(());
            }
            let o = s
                .merge(&user, &other, message.as_deref(), &choices)
                .map_err(err)?;
            outcome("merged", &o);
        }
        VersionCommand::Revert {
            project,
            revision,
            user,
            message,
            choose,
        } => {
            let mut s = open(&project)?;
            let o = s
                .revert(&user, &revision, message.as_deref(), &choices(&choose)?)
                .map_err(err)?;
            outcome("reverted", &o);
        }
        VersionCommand::Review {
            project,
            user,
            submit,
            into,
            reviewers,
            approve,
            reject,
            merge,
            note,
            choose,
        } => {
            let mut s = open(&project)?;
            if let Some(branch) = submit {
                let id = s
                    .submit_review(need(&user, "review")?, &branch, &into, &reviewers, &note)
                    .map_err(err)?;
                println!("review #{id}: {branch} into {into}");
                return Ok(());
            }
            if let Some(id) = approve.or(reject) {
                let state = s
                    .decide_review(need(&user, "review")?, id, approve.is_some(), &note)
                    .map_err(err)?;
                println!("review #{id} {}", state.as_str());
                return Ok(());
            }
            if let Some(id) = merge {
                let o = s
                    .merge_review(need(&user, "review")?, id, None, &choices(&choose)?)
                    .map_err(err)?;
                outcome(&format!("review #{id} merged"), &o);
                return Ok(());
            }
            let reviews = s.reviews().map_err(err)?;
            if reviews.is_empty() {
                println!("no review yet");
            }
            for r in reviews {
                println!(
                    "#{} {:<9} {} → {} at {} by {} ({}){}",
                    r.id,
                    r.state.as_str(),
                    r.branch,
                    r.target,
                    short(&r.revision),
                    r.author,
                    r.time.get(..16).unwrap_or(&r.time),
                    if r.reviewers.is_empty() {
                        String::new()
                    } else {
                        format!(" reviewers: {}", r.reviewers.join(", "))
                    }
                );
                if !r.note.is_empty() {
                    println!("    {}", r.note);
                }
                for n in &r.notes {
                    println!("    {} {}: {}", n.user, n.verdict, n.comment);
                }
            }
        }
        VersionCommand::Grant {
            project,
            by,
            subject,
            kinds,
            objects,
            area,
            note,
            revoke,
        } => {
            let s = open(&project)?;
            if let Some(id) = revoke {
                let g = s.revoke(need(&by, "grant")?, id).map_err(err)?;
                println!("grant {id} of {} revoked", g.subject);
                return Ok(());
            }
            if let Some(subject) = subject {
                let area = area.as_deref().map(parse_area).transpose()?;
                let g = Grant {
                    subject,
                    kinds,
                    objects,
                    area,
                    note,
                };
                let id = s.grant(need(&by, "grant")?, &g).map_err(err)?;
                println!("grant {id}: {} may edit {}", g.subject, g.describe());
                return Ok(());
            }
            let grants = s.grants().map_err(err)?;
            if grants.is_empty() {
                println!("no grants: every user edits what their role allows");
            }
            for g in grants {
                println!(
                    "{:>4}  {:<20} {}  (by {}, {})",
                    g.id,
                    g.grant.subject,
                    g.grant.describe(),
                    g.by,
                    g.time.get(..16).unwrap_or(&g.time)
                );
            }
        }
        VersionCommand::Bundle {
            project,
            export,
            out,
            import,
            as_branch,
            user,
        } => {
            let mut s = open(&project)?;
            match (export, out, import) {
                (Some(branch), Some(out), None) => {
                    let b = s.export_bundle(&branch).map_err(err)?;
                    let text = serde_json::to_string(&b).map_err(|e| e.to_string())?;
                    std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
                    println!(
                        "wrote {}: branch {branch}, {} revisions, {} blobs",
                        out.display(),
                        b.revisions.len(),
                        b.blobs.len()
                    );
                }
                (None, _, Some(file)) => {
                    let text = std::fs::read_to_string(&file)
                        .map_err(|e| format!("{}: {e}", file.display()))?;
                    let b: ohl_session::versions::Bundle = serde_json::from_str(&text)
                        .map_err(|e| format!("{}: {e}", file.display()))?;
                    let n = s
                        .import_bundle(need(&user, "bundle")?, &b, as_branch.as_deref())
                        .map_err(err)?;
                    println!(
                        "branch {} at {}: {n} new revision(s); merge it with ohl project merge",
                        as_branch.as_deref().unwrap_or(&b.branch),
                        short(&b.head)
                    );
                }
                _ => return Err("give --export BRANCH --out FILE, or --import FILE".into()),
            }
        }
    }
    Ok(())
}

fn parse_area(text: &str) -> Result<Vec<[f64; 2]>, String> {
    text.split(';')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let v: Vec<f64> = p
                .split(',')
                .map(|x| x.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| format!("--area: \"{p}\" is not X,Y"))?;
            match v[..] {
                [x, y] => Ok([x, y]),
                _ => Err(format!("--area: \"{p}\" is not X,Y")),
            }
        })
        .collect()
}

struct ImpactArgs {
    weather: Option<PathBuf>,
    case: Option<String>,
    temperature: f64,
    library: Option<PathBuf>,
    surface: Option<String>,
    max_sections: usize,
    time_limit: f64,
}

fn print_impact(
    s: &Session,
    a: &State,
    b: &State,
    d: &Diff,
    args: ImpactArgs,
) -> Result<(), String> {
    let cases = match &args.weather {
        Some(p) => {
            let set = ohl_weather::WeatherCases::load(p)?;
            let names: Vec<String> = args.case.iter().cloned().collect();
            let chosen = set.select(&names, &[])?;
            (set.cases.clone(), chosen[0].clone())
        }
        None => {
            let c = ohl_weather::WeatherCase::still(
                &format!("{} °C", args.temperature),
                ohl_core::units::Temperature::from_celsius(args.temperature),
            );
            (vec![c.clone()], c)
        }
    };
    let library = args
        .library
        .unwrap_or_else(|| ohl_core::files::shipped_library("cables"));
    let library = ohl_cables::Library::load(&library).map_err(|e| e.to_string())?;
    let surface = args
        .surface
        .or_else(|| s.setting("app.check_surface").ok().flatten());
    let terrain = match &surface {
        Some(name) => {
            let (points, triangles) = s.surface(name).map_err(err)?;
            Some(ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    let ground = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
    let ground_ref: Option<ohl_session::impact::Ground<'_>> = match &terrain {
        Some(_) => Some(&ground),
        None => None,
    };
    let started = std::time::Instant::now();
    let impact = ohl_session::impact::engineering_impact(
        a,
        b,
        d,
        &ohl_session::impact::ImpactOptions {
            case: &cases.1,
            cases: &cases.0,
            fallback_cables: Some(&library),
            ground: ground_ref,
            max_sections: args.max_sections,
            time_limit: Duration::from_secs_f64(args.time_limit.max(0.0)),
        },
    );
    println!(
        "engineering impact in \"{}\" ({} sections in {:.1?}{}):",
        cases.1.name,
        impact.sections.len(),
        started.elapsed(),
        surface
            .as_deref()
            .map(|n| format!(", ground: surface {n}"))
            .unwrap_or_else(|| ", no ground surface".into())
    );
    println!(
        "  {:<12} {:>22} {:>26} {:>22}",
        "section", "max sag, m", "max tension, kN", "min clearance, m"
    );
    let pair = |x: Option<f64>, y: Option<f64>, scale: f64| -> String {
        let f = |v: Option<f64>| v.map_or("—".to_owned(), |v| format!("{:.2}", v * scale));
        let delta = match (x, y) {
            (Some(x), Some(y)) => format!(" ({:+.2})", (y - x) * scale),
            _ => String::new(),
        };
        format!("{} → {}{delta}", f(x), f(y))
    };
    for si in &impact.sections {
        let (x, y) = (si.before, si.after);
        println!(
            "  {:<12} {:>22} {:>26} {:>22}",
            si.section,
            pair(x.map(|f| f.max_sag), y.map(|f| f.max_sag), 1.0),
            pair(x.map(|f| f.max_tension), y.map(|f| f.max_tension), 1e-3),
            pair(
                x.and_then(|f| f.min_clearance),
                y.and_then(|f| f.min_clearance),
                1.0
            ),
        );
        for e in &si.errors {
            println!("      {e}");
        }
    }
    if let Some(why) = &impact.stopped {
        println!("  stopped: {why}; {} sections not solved", impact.skipped);
    }
    Ok(())
}
