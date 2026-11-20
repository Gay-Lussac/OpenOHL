// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network rights …`: the network's users, groups, roles and
//! assignments for admins (ADR-0013, phase 7). Every change goes through
//! the rights' one function (`--user` is who acts: their rights decide)
//! and into the audit; a running server takes it before its next job.

use clap::Subcommand;
use ohl_session::rights::{Permission, Placement, Scope, Subject, Target, UserRights};
use ohl_store_pg::Network;
use ohl_store_pg::rights::{DbResolve, RightsCommand};

/// A scope on the command line (every option given must hold; none: the
/// whole network).
#[derive(clap::Args, Debug, Clone, Default)]
pub struct ScopeArgs {
    /// A line (model) by code.
    #[arg(long)]
    pub line: Option<String>,
    /// A route by id (its line found).
    #[arg(long)]
    pub route: Option<String>,
    /// With --to: the part of --route from this structure (snapped out to
    /// whole tension sections).
    #[arg(long)]
    pub from: Option<String>,
    /// See --from.
    #[arg(long)]
    pub to: Option<String>,
    /// A region: points "x,y;x,y;x,y;…" in the network's coordinates.
    #[arg(long)]
    pub region: Option<String>,
    /// An object kind (structure, section, …; meta for the settings,
    /// library for every library file).
    #[arg(long)]
    pub kind: Option<String>,
    /// One object by id.
    #[arg(long)]
    pub object: Option<String>,
    /// A library kind (cables, costs, …).
    #[arg(long)]
    pub library_kind: Option<String>,
    /// A library file (with --library-kind).
    #[arg(long)]
    pub library_file: Option<String>,
    /// A library entry (with --library-file).
    #[arg(long)]
    pub library_entry: Option<String>,
}

impl ScopeArgs {
    fn scope(&self) -> Result<Scope, String> {
        let region = match &self.region {
            Some(r) => Some(parse_region(r)?),
            None => None,
        };
        let s = Scope {
            line: self.line.clone(),
            route: self.route.clone(),
            from_structure: self.from.clone(),
            to_structure: self.to.clone(),
            region,
            kind: self.kind.clone(),
            object: self.object.clone(),
            library_kind: self.library_kind.clone(),
            library_file: self.library_file.clone(),
            library_entry: self.library_entry.clone(),
        };
        s.check()?;
        Ok(s)
    }
}

fn parse_region(text: &str) -> Result<Vec<[f64; 2]>, String> {
    text.split(';')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let (x, y) = p
                .split_once(',')
                .ok_or_else(|| format!("\"{p}\": a region's point is x,y"))?;
            let n = |v: &str| {
                v.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("\"{v}\": not a number"))
            };
            Ok([n(x)?, n(y)?])
        })
        .collect()
}

#[derive(Subcommand)]
pub enum RightsCommandArgs {
    /// The rights: roles, users, groups, assignments.
    List {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
    },
    /// Make the first network admin (only while the network has none).
    Bootstrap {
        #[arg(long)]
        database: Option<String>,
        /// Who becomes the network admin.
        #[arg(long)]
        user: String,
    },
    /// Add a user (network admins).
    AddUser {
        /// The user's name (as invitations name them).
        name: String,
        /// Display name.
        #[arg(long)]
        display: Option<String>,
        #[arg(long)]
        database: Option<String>,
        /// Who acts.
        #[arg(long)]
        user: String,
    },
    /// Disable a user (no rights at all; connections let go).
    Disable {
        name: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Enable a user again.
    Enable {
        name: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Add a group (network admins).
    AddGroup {
        name: String,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Remove a group with its assignments (network admins).
    RemoveGroup {
        name: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Make a user a member of a group (network admins).
    AddMember {
        group: String,
        member: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Take a user out of a group (network admins).
    RemoveMember {
        group: String,
        member: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Define a role or change its permissions (not a shipped one;
    /// network admins).
    PutRole {
        name: String,
        /// Permissions: see, edit, create, delete, review, export, lock,
        /// request, manage_rights (comma separated).
        #[arg(long)]
        permissions: String,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Remove a role (not a shipped one, not in use; network admins).
    RemoveRole {
        name: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Give a role to a user or group on a scope (or, network admins
    /// only, deny it: --deny; a deny without --role denies everything).
    Assign {
        /// user:NAME or group:NAME.
        #[arg(long)]
        to: String,
        /// The role.
        #[arg(long)]
        role: Option<String>,
        /// A deny (network admins only).
        #[arg(long)]
        deny: bool,
        #[command(flatten)]
        scope: ScopeArgs,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Take an assignment away (by its number, as listed).
    Unassign {
        id: i64,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Whether a user may do something on a target, and the assignments
    /// granting and denying it (who manages rights; in the audit).
    Explain {
        /// The user asked about.
        #[arg(long = "as")]
        who: String,
        /// The permission.
        #[arg(long)]
        permission: String,
        /// The line of the target (none: the network itself).
        #[arg(long)]
        line: Option<String>,
        /// The target's object kind (none with --line: the line whole).
        #[arg(long)]
        kind: Option<String>,
        /// The target's id (an object; a library file as KIND/FILE with
        /// --kind library).
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        database: Option<String>,
        /// Who asks (must manage rights somewhere).
        #[arg(long)]
        user: String,
    },
    /// What a user may see of each line (the server's own decision on
    /// what the database holds; within what the admin asking may see; in
    /// the audit).
    SeeAs {
        /// The user.
        #[arg(long = "as")]
        who: String,
        /// One line only.
        #[arg(long)]
        line: Option<String>,
        #[arg(long)]
        database: Option<String>,
        /// Who asks (must manage rights somewhere).
        #[arg(long)]
        user: String,
    },
    /// The audit (network admins): rights changes, refusals, exports,
    /// imports.
    Audit {
        /// From after this row on.
        #[arg(long, default_value_t = 0)]
        after: i64,
        /// Rows at most.
        #[arg(long, default_value_t = 200)]
        limit: i64,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
}

fn connect(db: Option<String>) -> Result<Network, String> {
    Network::connect(&crate::network_run::database(db)?).map_err(|e| e.to_string())
}

fn change(db: Option<String>, actor: &str, c: RightsCommand) -> Result<(), String> {
    let net = connect(db)?;
    let done = net.rights_command(actor, &c).map_err(|e| e.to_string())?;
    println!("{done}");
    Ok(())
}

/// The rights of `user` as the server compiles them (scopes resolved on
/// the database).
fn compiled(net: &Network, user: &str) -> Result<UserRights, String> {
    let data = net.rights().map_err(|e| e.to_string())?;
    Ok(data.compile(user, &DbResolve(net)))
}

/// `actor`'s rights, refused unless they manage rights somewhere.
fn manager(net: &Network, actor: &str, what: &str) -> Result<UserRights, String> {
    let me = compiled(net, actor)?;
    if !me.manages_anything() {
        let why = format!("{actor} manages no rights: may not {what}");
        let _ = net.audit(actor, "refused", what, &why);
        return Err(why);
    }
    Ok(me)
}

pub fn run(command: RightsCommandArgs) -> Result<(), String> {
    match command {
        RightsCommandArgs::List { database } => {
            let net = connect(database)?;
            let d = net.rights().map_err(|e| e.to_string())?;
            print!("{}", ohl_store_pg::rights::describe(&d));
            Ok(())
        }
        RightsCommandArgs::Bootstrap { database, user } => change(
            database,
            &user,
            RightsCommand::Bootstrap { user: user.clone() },
        ),
        RightsCommandArgs::AddUser {
            name,
            display,
            database,
            user,
        } => change(database, &user, RightsCommand::AddUser { name, display }),
        RightsCommandArgs::Disable {
            name,
            database,
            user,
        } => change(
            database,
            &user,
            RightsCommand::SetActive {
                name,
                active: false,
            },
        ),
        RightsCommandArgs::Enable {
            name,
            database,
            user,
        } => change(
            database,
            &user,
            RightsCommand::SetActive { name, active: true },
        ),
        RightsCommandArgs::AddGroup {
            name,
            note,
            database,
            user,
        } => change(database, &user, RightsCommand::AddGroup { name, note }),
        RightsCommandArgs::RemoveGroup {
            name,
            database,
            user,
        } => change(database, &user, RightsCommand::DeleteGroup { name }),
        RightsCommandArgs::AddMember {
            group,
            member,
            database,
            user,
        } => change(
            database,
            &user,
            RightsCommand::AddMember {
                group,
                user: member,
            },
        ),
        RightsCommandArgs::RemoveMember {
            group,
            member,
            database,
            user,
        } => change(
            database,
            &user,
            RightsCommand::RemoveMember {
                group,
                user: member,
            },
        ),
        RightsCommandArgs::PutRole {
            name,
            permissions,
            note,
            database,
            user,
        } => {
            let permissions = permissions
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| Permission::parse(p).ok_or_else(|| format!("\"{p}\": no such permission")))
                .collect::<Result<Vec<_>, _>>()?;
            change(
                database,
                &user,
                RightsCommand::PutRole {
                    name,
                    permissions,
                    note,
                },
            )
        }
        RightsCommandArgs::RemoveRole {
            name,
            database,
            user,
        } => change(database, &user, RightsCommand::DeleteRole { name }),
        RightsCommandArgs::Assign {
            to,
            role,
            deny,
            scope,
            note,
            database,
            user,
        } => change(
            database,
            &user,
            RightsCommand::Assign {
                subject: Subject::parse(&to)?,
                role,
                deny,
                scope: scope.scope()?,
                note,
            },
        ),
        RightsCommandArgs::Unassign { id, database, user } => {
            change(database, &user, RightsCommand::Unassign { id })
        }
        RightsCommandArgs::Explain {
            who,
            permission,
            line,
            kind,
            id,
            database,
            user,
        } => {
            let net = connect(database)?;
            let me = manager(&net, &user, &format!("explain the rights of {who}"))?;
            let p = Permission::parse(&permission)
                .ok_or_else(|| format!("\"{permission}\": no such permission"))?;
            let them = compiled(&net, &who)?;
            let kind = match &kind {
                Some(k) => Some(ohl_session::Kind::parse(k).map_err(|e| e.to_string())?),
                None => None,
            };
            let (bounds, routes) = match (&line, kind, &id) {
                (Some(l), Some(k), Some(i)) => net
                    .placement_of(l, k.as_str(), i)
                    .map_err(|e| e.to_string())?,
                _ => (None, Vec::new()),
            };
            let target = match (&line, kind, &id) {
                (None, _, _) => Target::Network,
                (Some(l), None, _) => Target::Line(l),
                (Some(l), Some(k), Some(i)) => Target::object(
                    l,
                    k,
                    i,
                    Placement {
                        bounds,
                        routes: &routes,
                    },
                ),
                (Some(_), Some(_), None) => return Err("--kind needs --id".into()),
            };
            // An admin learns only about what they may see themselves.
            if !matches!(target, Target::Network) && !me.allows(Permission::See, &target) {
                return Err(format!("{user} may not see that target"));
            }
            let (ok, grants, denies) = them.explain(p, &target);
            let _ = net.audit(
                &user,
                "explain",
                &format!("user:{who}"),
                &format!("{} on {line:?}/{kind:?}/{id:?}", p.as_str()),
            );
            println!(
                "{who} {} {}{}",
                if ok { "may" } else { "may not" },
                p.as_str(),
                if them.active() {
                    ""
                } else {
                    " (not an active user of the network)"
                }
            );
            println!("granted by assignments: {grants:?}");
            println!("denied by assignments: {denies:?}");
            if p.needs_see() && !them.allows(Permission::See, &target) {
                println!("({} needs see, which is not given there)", p.as_str());
            }
            Ok(())
        }
        RightsCommandArgs::SeeAs {
            who,
            line,
            database,
            user,
        } => {
            let net = connect(database)?;
            let me = manager(&net, &user, &format!("see as {who}"))?;
            let both = me.and(compiled(&net, &who)?);
            let _ = net.audit(&user, "see_as", &format!("user:{who}"), "the command line");
            let lines: Vec<String> = match line {
                Some(l) => vec![l],
                None => net
                    .lines()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|l| l.id)
                    .collect(),
            };
            for l in lines {
                if !both.sees_some_of_line(&l) {
                    continue;
                }
                let counts = net.see_counts(&l, &both).map_err(|e| e.to_string())?;
                println!(
                    "line {l}{}: {}",
                    if both.sees_whole_line(&l) {
                        " (whole)"
                    } else {
                        ""
                    },
                    counts
                        .iter()
                        .map(|(k, (seen, all))| format!("{k} {seen} of {all}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(())
        }
        RightsCommandArgs::Audit {
            after,
            limit,
            database,
            user,
        } => {
            let net = connect(database)?;
            let me = compiled(&net, &user)?;
            if !me.allows(Permission::ManageRights, &Target::Network) {
                let why = format!("{user} may not read the audit: network admins only");
                let _ = net.audit(&user, "refused", "audit", &why);
                return Err(why);
            }
            for (id, [time, who, action, target, detail]) in
                net.audit_after(after, limit).map_err(|e| e.to_string())?
            {
                println!("{id}\t{time}\t{who}\t{action}\t{target}\t{detail}");
            }
            Ok(())
        }
    }
}
