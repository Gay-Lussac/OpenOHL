// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The network's rights as the database keeps them (ADR-0013, phase 7;
//! schema 4): users, groups and their members, roles and their
//! permissions, assignments — read whole ([`Network::rights`]) for the one
//! function deciding ([`ohl_session::rights`]), changed by
//! [`RightsCommand`]s that go through the same function
//! ([`Network::rights_command`]):
//!
//! - users, groups, members and roles: network admins only (a role's
//!   permissions or a membership reach every scope its assignments name);
//! - an assignment: by whoever manages rights on a scope holding its scope
//!   ([`ohl_session::rights::UserRights::manages`]) and holds every
//!   permission of the role given there; a **deny** by network admins only;
//! - the shipped roles cannot be changed or removed; a role in use cannot
//!   be removed; the last network admin cannot be taken away (nor
//!   disabled);
//! - the first network admin by [`RightsCommand::Bootstrap`], only while
//!   the network has none.
//!
//! Each change is one transaction (serialised by an advisory lock, so two
//! admins at once do not both pass a check the other's change breaks)
//! with its audit row (before and after) and the rights' revision moved
//! on (`net_meta` `rights_rev`): the server reads the rights again before
//! its next job. A refused change is in the audit too.
//!
//! Rights changes need no writer's lock: they are not commands of a model
//! (no log entry), and an admin changes them while the server runs.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ohl_session::Kind;
use ohl_session::rights::{
    Assignment, NoModels, Permission, Perms, Resolve, RightsData, Scope, Subject, Target,
};

use crate::{NetError, Network, WRITER_LOCK, lock};

/// The advisory lock serialising rights changes.
const RIGHTS_LOCK: i64 = WRITER_LOCK + 2;

/// Longest user, group or role name, characters.
pub const NAME_MAX: usize = 64;

/// A change of the network's rights (defined beside the rights' one
/// function since 2026-11-28, so that the server's admin messages carry it:
/// [`ohl_session::rights::RightsCommand`]).
pub use ohl_session::rights::RightsCommand;

fn refused(m: impl Into<String>) -> NetError {
    NetError::Refused(m.into())
}

fn check_name(what: &str, name: &str) -> Result<(), NetError> {
    let ok = !name.trim().is_empty()
        && name.trim() == name
        && name.chars().count() <= NAME_MAX
        && !name.chars().any(|c| c.is_control() || c == ':');
    if ok {
        Ok(())
    } else {
        Err(refused(format!(
            "\"{name}\": a {what} name of 1 to {NAME_MAX} characters, no ':' or control characters, \
             no spaces around"
        )))
    }
}

/// Reads the rights from a connection.
pub(crate) fn read(c: &mut crate::Conn) -> Result<RightsData, postgres::Error> {
    let mut d = RightsData::default();
    for r in c.query("SELECT role, permission FROM role_permission", &[])? {
        let (role, p): (String, String) = (r.get(0), r.get(1));
        let e = d.roles.entry(role).or_insert((Perms::NONE, false));
        if let Some(p) = Permission::parse(&p) {
            e.0 = e.0.union(Perms::of(&[p]));
        }
    }
    for r in c.query("SELECT name, shipped FROM role", &[])? {
        let e = d.roles.entry(r.get(0)).or_insert((Perms::NONE, false));
        e.1 = r.get(1);
    }
    for r in c.query("SELECT name, active FROM net_user", &[])? {
        d.users.insert(r.get(0), r.get(1));
    }
    for r in c.query("SELECT name FROM net_group", &[])? {
        d.groups.insert(r.get(0), BTreeSet::new());
    }
    for r in c.query("SELECT grp, member FROM net_member", &[])? {
        d.groups.entry(r.get(0)).or_default().insert(r.get(1));
    }
    for r in c.query(
        "SELECT id, subject_kind, subject, role, deny, line, route, from_structure, to_structure,
             region, kind, object, lib_kind, lib_file, lib_entry, by_user, note
         FROM assignment ORDER BY id",
        &[],
    )? {
        let kind: String = r.get(1);
        let name: String = r.get(2);
        let subject = if kind == "group" {
            Subject::Group(name)
        } else {
            Subject::User(name)
        };
        let region: Option<String> = r.get(9);
        d.assignments.push(Assignment {
            id: r.get(0),
            subject,
            role: r.get(3),
            deny: r.get(4),
            scope: Scope {
                line: r.get(5),
                route: r.get(6),
                from_structure: r.get(7),
                to_structure: r.get(8),
                // A region not read back is no region: the assignment then
                // covers more (a grant) or less (a deny) than meant — so
                // refused when written, never left so.
                region: region.and_then(|t| serde_json::from_str(&t).ok()),
                kind: r.get(10),
                object: r.get(11),
                library_kind: r.get(12),
                library_file: r.get(13),
                library_entry: r.get(14),
            },
            by: r.get(15),
            note: r.get(16),
        });
    }
    d.rev = rev_of(c)?;
    Ok(d)
}

fn rev_of(c: &mut crate::Conn) -> Result<i64, postgres::Error> {
    Ok(
        c.query_opt("SELECT value FROM net_meta WHERE key = 'rights_rev'", &[])?
            .and_then(|r| r.get::<_, String>(0).parse().ok())
            .unwrap_or(0),
    )
}

/// A region as PostGIS text.
fn wkt(region: &[[f64; 2]]) -> String {
    let mut pts: Vec<String> = region
        .iter()
        .map(|p| format!("{:?} {:?}", p[0], p[1]))
        .collect();
    pts.push(format!("{:?} {:?}", region[0][0], region[0][1]));
    format!("POLYGON(({}))", pts.join(", "))
}

/// The kinds whose ids are unique in the network (an object scope's line
/// is found from its id).
const UNIQUE_KINDS: [&str; 7] = [
    "route",
    "alignment",
    "alignment_point",
    "structure",
    "section",
    "jumper",
    "spacer",
];

impl Network {
    /// The rights, read at one moment (users, groups, roles, assignments
    /// and their revision).
    pub fn rights(&self) -> Result<RightsData, NetError> {
        let mut c = lock(&self.conn);
        if !c.steps.is_empty() {
            return Ok(read(&mut c)?);
        }
        c.client
            .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")?;
        let d = read(&mut c);
        let _ = c.client.batch_execute("COMMIT");
        Ok(d?)
    }

    /// The rights' revision (each change adds one).
    pub fn rights_rev(&self) -> Result<i64, NetError> {
        Ok(rev_of(&mut lock(&self.conn))?)
    }

    /// The model holding element `id` of a kind unique in the network (any
    /// of them when `kind` is none), with its kind.
    pub fn line_of(
        &self,
        kind: Option<&str>,
        id: &str,
    ) -> Result<Option<(String, String)>, NetError> {
        let kinds: Vec<&str> = match kind {
            Some(k) => vec![k],
            None => UNIQUE_KINDS.to_vec(),
        };
        Ok(lock(&self.conn)
            .query_opt(
                "SELECT line_id, kind FROM object
                 WHERE branch = 'main' AND gone = 0 AND id = $1 AND kind = ANY($2) LIMIT 1",
                &[&id, &kinds],
            )?
            .map(|r| (r.get(0), r.get(1))))
    }

    /// Where element `kind`/`id` of model `line`'s main branch is: its
    /// plan bounds and its routes (as the server keeps them).
    pub fn placement_of(
        &self,
        line: &str,
        kind: &str,
        id: &str,
    ) -> Result<(Option<ohl_core::grid::Bounds>, Vec<String>), NetError> {
        let mut c = lock(&self.conn);
        let bounds = c
            .query_opt(
                "SELECT x0, y0, x1, y1 FROM spatial
                 WHERE line_id = $1 AND branch = 'main' AND kind = $2 AND id = $3",
                &[&line, &kind, &id],
            )?
            .map(|r| ohl_core::grid::Bounds::new([r.get(0), r.get(1)], [r.get(2), r.get(3)]));
        let routes = c
            .query_opt(
                "SELECT routes FROM object
                 WHERE line_id = $1 AND branch = 'main' AND kind = $2 AND id = $3",
                &[&line, &kind, &id],
            )?
            .and_then(|r| r.get::<_, Option<Vec<String>>>(0))
            .unwrap_or_default();
        Ok((bounds, routes))
    }

    /// Applies `command` of `actor`: checked by the rights' one function
    /// against the rights as they are (read in the same transaction),
    /// written, audited, the revision moved on; refused (and audited)
    /// otherwise. Returns what was done, in words.
    pub fn rights_command(&self, actor: &str, command: &RightsCommand) -> Result<String, NetError> {
        let done = self.rights_change(actor, command);
        if let Err(e) = &done {
            let _ = self.audit(
                actor,
                "refused",
                &command.action(),
                &format!(
                    "{e}; {}",
                    serde_json::to_string(command).expect("serialises")
                ),
            );
        }
        done
    }

    /// (Decided structurally — who manages a scope holding it, network
    /// admins —, so without resolving scopes on the models: the
    /// connection is held meanwhile.)
    fn rights_change(&self, actor: &str, command: &RightsCommand) -> Result<String, NetError> {
        let mut c = lock(&self.conn);
        if !c.steps.is_empty() {
            return Err(refused(
                "a rights change during a unit of work of the same connection",
            ));
        }
        c.client.batch_execute("BEGIN")?;
        let out = (|| -> Result<(String, String, Option<String>, Option<String>), NetError> {
            c.execute("SELECT pg_advisory_xact_lock($1)", &[&RIGHTS_LOCK])?;
            let data = read(&mut c)?;
            let me = data.compile(actor, &NoModels);
            let network_admin = me.allows(Permission::ManageRights, &Target::Network);
            let admin_only = |what: &str| -> Result<(), NetError> {
                if network_admin {
                    Ok(())
                } else {
                    Err(refused(format!(
                        "{actor} may not {what}: network admins only"
                    )))
                }
            };
            let json = |v: &serde_json::Value| Some(v.to_string());
            Ok(match command {
                RightsCommand::Bootstrap { user } => {
                    check_name("user", user)?;
                    if !data.network_admins().is_empty() {
                        return Err(refused(
                            "the network has a network admin: ask one (bootstrap only makes the \
                             first)",
                        ));
                    }
                    c.execute(
                        "INSERT INTO net_user (name, display) VALUES ($1, $1)
                         ON CONFLICT (name) DO UPDATE SET active = true",
                        &[user],
                    )?;
                    let id: i64 = c
                        .query_one(
                            "INSERT INTO assignment (subject_kind, subject, role, scope_kind,
                                 by_user, note)
                             VALUES ('user', $1, 'network_admin', 'network', $2,
                                 'the first network admin')
                             RETURNING id",
                            &[user, &actor],
                        )?
                        .get(0);
                    (
                        format!("{user} is the network's first network admin (assignment #{id})"),
                        format!("user:{user}"),
                        None,
                        json(&serde_json::json!({"assignment": id, "role": "network_admin"})),
                    )
                }
                RightsCommand::AddUser { name, display } => {
                    admin_only("add users")?;
                    check_name("user", name)?;
                    let display = display.clone().unwrap_or_else(|| name.clone());
                    let before = data
                        .users
                        .get(name)
                        .map(|a| serde_json::json!({"active": a}));
                    c.execute(
                        "INSERT INTO net_user (name, display) VALUES ($1, $2)
                         ON CONFLICT (name) DO UPDATE SET display = excluded.display",
                        &[name, &display],
                    )?;
                    (
                        format!("user {name} added"),
                        format!("user:{name}"),
                        before.map(|b| b.to_string()),
                        json(&serde_json::json!({"display": display})),
                    )
                }
                RightsCommand::SetActive { name, active } => {
                    admin_only("disable or enable users")?;
                    let Some(was) = data.users.get(name) else {
                        return Err(refused(format!("no user {name}")));
                    };
                    if !active {
                        let mut after = data.clone();
                        after.users.insert(name.clone(), false);
                        if after.network_admins().is_empty() {
                            return Err(refused(format!(
                                "{name} is the last network admin: make another first"
                            )));
                        }
                    }
                    c.execute(
                        "UPDATE net_user SET active = $2 WHERE name = $1",
                        &[name, active],
                    )?;
                    (
                        format!(
                            "user {name} {}",
                            if *active { "enabled" } else { "disabled" }
                        ),
                        format!("user:{name}"),
                        json(&serde_json::json!({"active": was})),
                        json(&serde_json::json!({"active": active})),
                    )
                }
                RightsCommand::AddGroup { name, note } => {
                    admin_only("add groups")?;
                    check_name("group", name)?;
                    if data.groups.contains_key(name) {
                        return Err(refused(format!("the network has a group {name} already")));
                    }
                    c.execute(
                        "INSERT INTO net_group (name, note) VALUES ($1, $2)",
                        &[name, note],
                    )?;
                    (
                        format!("group {name} added"),
                        format!("group:{name}"),
                        None,
                        json(&serde_json::json!({"note": note})),
                    )
                }
                RightsCommand::DeleteGroup { name } => {
                    admin_only("remove groups")?;
                    let Some(members) = data.groups.get(name) else {
                        return Err(refused(format!("no group {name}")));
                    };
                    let mut after = data.clone();
                    after.groups.remove(name);
                    after
                        .assignments
                        .retain(|a| a.subject != Subject::Group(name.clone()));
                    if after.network_admins().is_empty() {
                        return Err(refused(format!(
                            "removing group {name} leaves the network without a network admin"
                        )));
                    }
                    let gone = c.execute(
                        "DELETE FROM assignment WHERE subject_kind = 'group' AND subject = $1",
                        &[name],
                    )?;
                    c.execute("DELETE FROM net_group WHERE name = $1", &[name])?;
                    (
                        format!("group {name} removed with its {gone} assignments"),
                        format!("group:{name}"),
                        json(&serde_json::json!({"members": members})),
                        None,
                    )
                }
                RightsCommand::AddMember { group, user } => {
                    admin_only("change groups")?;
                    if !data.groups.contains_key(group) {
                        return Err(refused(format!("no group {group}")));
                    }
                    if !data.users.contains_key(user) {
                        return Err(refused(format!("no user {user}")));
                    }
                    c.execute(
                        "INSERT INTO net_member (grp, member) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                        &[group, user],
                    )?;
                    (
                        format!("{user} is a member of {group}"),
                        format!("group:{group}"),
                        None,
                        json(&serde_json::json!({"member": user})),
                    )
                }
                RightsCommand::RemoveMember { group, user } => {
                    admin_only("change groups")?;
                    if !data.groups.get(group).is_some_and(|m| m.contains(user)) {
                        return Err(refused(format!("{user} is not a member of {group}")));
                    }
                    let mut after = data.clone();
                    if let Some(m) = after.groups.get_mut(group) {
                        m.remove(user);
                    }
                    if after.network_admins().is_empty() {
                        return Err(refused(format!(
                            "{user} is the last network admin (through {group})"
                        )));
                    }
                    c.execute(
                        "DELETE FROM net_member WHERE grp = $1 AND member = $2",
                        &[group, user],
                    )?;
                    (
                        format!("{user} is no longer a member of {group}"),
                        format!("group:{group}"),
                        json(&serde_json::json!({"member": user})),
                        None,
                    )
                }
                RightsCommand::PutRole {
                    name,
                    permissions,
                    note,
                } => {
                    admin_only("define roles")?;
                    check_name("role", name)?;
                    if data.roles.get(name).is_some_and(|(_, shipped)| *shipped) {
                        return Err(refused(format!(
                            "{name} is a shipped role: define a role of another name"
                        )));
                    }
                    let perms = Perms::of(permissions);
                    let before = data.roles.get(name).map(|(p, _)| p.describe());
                    c.execute(
                        "INSERT INTO role (name, shipped, note) VALUES ($1, false, $2)
                         ON CONFLICT (name) DO UPDATE SET note = excluded.note",
                        &[name, note],
                    )?;
                    c.execute("DELETE FROM role_permission WHERE role = $1", &[name])?;
                    for p in perms.list() {
                        c.execute(
                            "INSERT INTO role_permission (role, permission) VALUES ($1, $2)",
                            &[name, &p.as_str()],
                        )?;
                    }
                    (
                        format!("role {name}: {}", perms.describe()),
                        format!("role:{name}"),
                        before.map(|b| serde_json::json!({ "permissions": b }).to_string()),
                        json(&serde_json::json!({"permissions": perms.describe()})),
                    )
                }
                RightsCommand::DeleteRole { name } => {
                    admin_only("remove roles")?;
                    match data.roles.get(name) {
                        None => return Err(refused(format!("no role {name}"))),
                        Some((_, true)) => {
                            return Err(refused(format!("{name} is a shipped role")));
                        }
                        _ => {}
                    }
                    let used: Vec<i64> = data
                        .assignments
                        .iter()
                        .filter(|a| a.role.as_deref() == Some(name))
                        .map(|a| a.id)
                        .collect();
                    if !used.is_empty() {
                        return Err(refused(format!(
                            "role {name} is given by {} assignments (#{}): take them away first",
                            used.len(),
                            used.iter()
                                .map(i64::to_string)
                                .collect::<Vec<_>>()
                                .join(", #")
                        )));
                    }
                    let perms = data.roles[name].0.describe();
                    c.execute("DELETE FROM role WHERE name = $1", &[name])?;
                    (
                        format!("role {name} removed"),
                        format!("role:{name}"),
                        json(&serde_json::json!({"permissions": perms})),
                        None,
                    )
                }
                RightsCommand::Assign {
                    subject,
                    role,
                    deny,
                    scope,
                    note,
                } => {
                    check_name(subject.kind(), subject.name())?;
                    match subject {
                        Subject::User(u) if !data.users.contains_key(u) => {
                            return Err(refused(format!("no user {u}: add the user first")));
                        }
                        Subject::Group(g) if !data.groups.contains_key(g) => {
                            return Err(refused(format!("no group {g}: add the group first")));
                        }
                        _ => {}
                    }
                    let perms = match role {
                        Some(r) => match data.roles.get(r) {
                            Some((p, _)) => *p,
                            None => return Err(refused(format!("no role {r}"))),
                        },
                        None if *deny => Perms::ALL,
                        None => {
                            return Err(refused(
                                "an assignment gives a role (a deny may name none)",
                            ));
                        }
                    };
                    scope.check().map_err(refused)?;
                    let scope = self.complete_scope(&mut c, scope)?;
                    if *deny {
                        admin_only("set a deny")?;
                    } else if !me.manages(&scope) {
                        return Err(refused(format!(
                            "{actor} does not manage rights on {}",
                            scope.describe()
                        )));
                    }
                    // No one gives more than they hold there.
                    if !network_admin && !mine_holds(&data, actor, &scope, perms) {
                        return Err(refused(format!(
                            "{actor} may give only permissions held on {} ({} asked)",
                            scope.describe(),
                            perms.describe()
                        )));
                    }
                    let region = scope
                        .region
                        .as_ref()
                        .map(|r| serde_json::to_string(r).expect("serialises"));
                    let area = scope.region.as_ref().map(|r| wkt(r));
                    let id: i64 = c
                        .query_one(
                            "INSERT INTO assignment (subject_kind, subject, role, deny, scope_kind,
                                 line, route, from_structure, to_structure, area, region, kind,
                                 object, lib_kind, lib_file, lib_entry, by_user, note)
                             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, ST_GeomFromText($10),
                                 $11, $12, $13, $14, $15, $16, $17, $18)
                             RETURNING id",
                            &[
                                &subject.kind(),
                                &subject.name(),
                                role,
                                deny,
                                &scope.scope_kind(),
                                &scope.line,
                                &scope.route,
                                &scope.from_structure,
                                &scope.to_structure,
                                &area,
                                &region,
                                &scope.kind,
                                &scope.object,
                                &scope.library_kind,
                                &scope.library_file,
                                &scope.library_entry,
                                &actor,
                                note,
                            ],
                        )?
                        .get(0);
                    let a = Assignment {
                        id,
                        subject: subject.clone(),
                        role: role.clone(),
                        deny: *deny,
                        scope,
                        by: actor.to_owned(),
                        note: note.clone(),
                    };
                    (
                        format!("assignment {}", a.describe()),
                        format!("assignment:{id}"),
                        None,
                        json(&serde_json::to_value(&a).expect("serialises")),
                    )
                }
                RightsCommand::Unassign { id } => {
                    let Some(a) = data.assignments.iter().find(|a| a.id == *id) else {
                        return Err(refused(format!("no assignment #{id}")));
                    };
                    if a.deny {
                        admin_only("take a deny away")?;
                    } else if !me.manages(&a.scope) {
                        return Err(refused(format!(
                            "{actor} does not manage rights on {}",
                            a.scope.describe()
                        )));
                    }
                    let mut after = data.clone();
                    after.assignments.retain(|x| x.id != *id);
                    if after.network_admins().is_empty() {
                        return Err(refused(
                            "this assignment makes the last network admin: make another first",
                        ));
                    }
                    c.execute("DELETE FROM assignment WHERE id = $1", &[id])?;
                    (
                        format!("assignment {} taken away", a.describe()),
                        format!("assignment:{id}"),
                        json(&serde_json::to_value(a).expect("serialises")),
                        None,
                    )
                }
                RightsCommand::ReissueInvitation { name } => {
                    admin_only("reissue invitations")?;
                    if !data.users.contains_key(name) {
                        return Err(refused(format!("no user {name}")));
                    }
                    let generation: i32 = c
                        .query_one(
                            "UPDATE net_user SET invitation = invitation + 1 WHERE name = $1
                             RETURNING invitation",
                            &[name],
                        )?
                        .get(0);
                    (
                        format!(
                            "{name}'s personal invitation reissued (generation {generation}): \
                             the older ones are refused"
                        ),
                        format!("user:{name}"),
                        json(&serde_json::json!({"invitation": generation - 1})),
                        json(&serde_json::json!({"invitation": generation})),
                    )
                }
                RightsCommand::MapIdentity {
                    issuer,
                    subject,
                    user,
                } => {
                    admin_only("map sign-in identities")?;
                    if !data.users.contains_key(user) {
                        return Err(refused(format!("no user {user}")));
                    }
                    if issuer.trim().is_empty()
                        || subject.trim().is_empty()
                        || issuer.len() > 1024
                        || subject.len() > 255
                    {
                        return Err(refused(
                            "an identity: an issuer (at most 1024 bytes) and a subject (at most \
                             255)",
                        ));
                    }
                    let before: Option<String> = c
                        .query_opt(
                            "SELECT user_name FROM net_identity WHERE issuer = $1 AND subject = $2",
                            &[issuer, subject],
                        )?
                        .map(|r| r.get(0));
                    c.execute(
                        "INSERT INTO net_identity (issuer, subject, user_name, by_user)
                         VALUES ($1, $2, $3, $4)
                         ON CONFLICT (issuer, subject) DO UPDATE
                             SET user_name = excluded.user_name, by_user = excluded.by_user,
                                 time = clock_timestamp()",
                        &[issuer, subject, user, &actor],
                    )?;
                    (
                        format!("{subject} of {issuer} signs in as {user}"),
                        format!("identity:{issuer} {subject}"),
                        before.map(|u| serde_json::json!({"user": u}).to_string()),
                        json(&serde_json::json!({"user": user})),
                    )
                }
                RightsCommand::UnmapIdentity { issuer, subject } => {
                    admin_only("map sign-in identities")?;
                    let before: Option<String> = c
                        .query_opt(
                            "DELETE FROM net_identity WHERE issuer = $1 AND subject = $2
                             RETURNING user_name",
                            &[issuer, subject],
                        )?
                        .map(|r| r.get(0));
                    let Some(before) = before else {
                        return Err(refused(format!("no identity {subject} of {issuer}")));
                    };
                    (
                        format!("{subject} of {issuer} no longer signs in (was {before})"),
                        format!("identity:{issuer} {subject}"),
                        json(&serde_json::json!({"user": before})),
                        None,
                    )
                }
            })
        })();
        let out = out.and_then(|(text, target, before, after)| {
            c.execute(
                "INSERT INTO audit (user_name, action, target, before, after, detail)
                 VALUES ($1, $2, $3, ($4::text)::jsonb, ($5::text)::jsonb, $6)",
                &[&actor, &command.action(), &target, &before, &after, &text],
            )?;
            c.execute(
                "INSERT INTO net_meta (key, value) VALUES ('rights_rev', '1')
                 ON CONFLICT (key) DO UPDATE SET value = (net_meta.value::bigint + 1)::text",
                &[],
            )?;
            Ok(text)
        });
        match out {
            Ok(text) => {
                c.client.batch_execute("COMMIT")?;
                Ok(text)
            }
            Err(e) => {
                let _ = c.client.batch_execute("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Phase 13: each user's personal invitation's generation (0: never
    /// reissued).
    pub fn invitation_generations(&self) -> Result<BTreeMap<String, i32>, NetError> {
        Ok(lock(&self.conn)
            .query("SELECT name, invitation FROM net_user", &[])?
            .into_iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect())
    }

    /// Phase 13: the user `subject` of `issuer` signs in as, if mapped.
    pub fn identity_user(&self, issuer: &str, subject: &str) -> Result<Option<String>, NetError> {
        Ok(lock(&self.conn)
            .query_opt(
                "SELECT user_name FROM net_identity WHERE issuer = $1 AND subject = $2",
                &[&issuer, &subject],
            )?
            .map(|r| r.get(0)))
    }

    /// Phase 13: every identity mapped: issuer, subject, user.
    pub fn identities(&self) -> Result<Vec<[String; 3]>, NetError> {
        Ok(lock(&self.conn)
            .query(
                "SELECT issuer, subject, user_name FROM net_identity ORDER BY 3, 1, 2",
                &[],
            )?
            .into_iter()
            .map(|r| [r.get(0), r.get(1), r.get(2)])
            .collect())
    }

    /// A route, part or object scope with its line (ids are unique in the
    /// network); refused when what it names is not there, or on another
    /// line than the one given.
    fn complete_scope(&self, c: &mut crate::Conn, scope: &Scope) -> Result<Scope, NetError> {
        let mut s = scope.clone();
        let find =
            |c: &mut crate::Conn, kinds: Vec<&str>, id: &str| -> Result<Option<String>, NetError> {
                Ok(c.query_opt(
                    "SELECT line_id FROM object
                     WHERE branch = 'main' AND gone = 0 AND id = $1 AND kind = ANY($2) LIMIT 1",
                    &[&id, &kinds],
                )?
                .map(|r| r.get(0)))
            };
        if let Some(route) = &scope.route {
            let line = find(c, vec!["route"], route)?
                .ok_or_else(|| refused(format!("no route {route} in the network")))?;
            for st in [&scope.from_structure, &scope.to_structure]
                .into_iter()
                .flatten()
            {
                let on = find(c, vec!["structure"], st)?;
                if on.as_deref() != Some(line.as_str()) {
                    return Err(refused(format!("no structure {st} on line {line}")));
                }
            }
            s.line = Some(match &scope.line {
                Some(l) if *l != line => {
                    return Err(refused(format!("route {route} is on line {line}, not {l}")));
                }
                _ => line,
            });
        } else if let Some(object) = &scope.object {
            let kinds: Vec<&str> = match &scope.kind {
                Some(k) if UNIQUE_KINDS.contains(&k.as_str()) => vec![k.as_str()],
                Some(_) if scope.line.is_some() => vec![],
                Some(k) => {
                    return Err(refused(format!(
                        "an object of kind {k} names its line too (its ids are the line's own)"
                    )));
                }
                None => UNIQUE_KINDS.to_vec(),
            };
            if !kinds.is_empty() {
                let line = find(c, kinds, object)?
                    .ok_or_else(|| refused(format!("no object {object} in the network")))?;
                s.line = Some(match &scope.line {
                    Some(l) if *l != line => {
                        return Err(refused(format!("{object} is on line {line}, not {l}")));
                    }
                    _ => line,
                });
            }
        }
        if let Some(l) = &s.line
            && c.query_opt("SELECT 1 FROM line WHERE id = $1", &[l])?
                .is_none()
        {
            return Err(refused(format!("the network has no line {l}")));
        }
        Ok(s)
    }

    /// What `rights` may see of model `line`'s main branch, by object
    /// kind: (seen, all) — the server's decision on what the database
    /// holds ("see as user" on the command line).
    pub fn see_counts(
        &self,
        line: &str,
        rights: &ohl_session::rights::UserRights,
    ) -> Result<std::collections::BTreeMap<String, (usize, usize)>, NetError> {
        let rows = lock(&self.conn).query(
            "SELECT o.kind, o.id, o.routes, s.x0, s.y0, s.x1, s.y1
             FROM object o LEFT JOIN spatial s
               ON s.line_id = o.line_id AND s.branch = o.branch AND s.kind = o.kind AND s.id = o.id
             WHERE o.line_id = $1 AND o.branch = 'main' AND o.gone = 0",
            &[&line],
        )?;
        let mut out: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
        for r in rows {
            let kind: String = r.get(0);
            let id: String = r.get(1);
            let routes: Vec<String> = r.get::<_, Option<Vec<String>>>(2).unwrap_or_default();
            let x0: Option<f64> = r.get(3);
            let bounds =
                x0.map(|x0| ohl_core::grid::Bounds::new([x0, r.get(4)], [r.get(5), r.get(6)]));
            let Ok(k) = Kind::parse(&kind) else {
                continue;
            };
            let t = Target::object(
                line,
                k,
                &id,
                ohl_session::rights::Placement {
                    bounds,
                    routes: &routes,
                },
            );
            let e = out.entry(kind).or_default();
            e.1 += 1;
            if rights.allows(Permission::See, &t) {
                e.0 += 1;
            }
        }
        Ok(out)
    }

    /// Every route of the models' main branches: (line, route).
    pub fn route_list(&self) -> Result<Vec<(String, String)>, NetError> {
        Ok(lock(&self.conn)
            .query(
                "SELECT line_id, id FROM object
                 WHERE branch = 'main' AND gone = 0 AND kind = 'route' ORDER BY line_id, id",
                &[],
            )?
            .into_iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect())
    }

    /// The structures of route `route` of model `line` (by id).
    pub fn structures_on_route(&self, line: &str, route: &str) -> Result<Vec<String>, NetError> {
        Ok(lock(&self.conn)
            .query(
                "SELECT id FROM object
                 WHERE line_id = $1 AND branch = 'main' AND gone = 0 AND kind = 'structure'
                   AND routes @> ARRAY[$2]::text[]
                 ORDER BY id",
                &[&line, &route],
            )?
            .into_iter()
            .map(|r| r.get(0))
            .collect())
    }

    /// The audit rows from number `after` on (oldest first, at most
    /// `limit`): number, time, user, action, target, detail.
    pub fn audit_after(&self, after: i64, limit: i64) -> Result<Vec<(i64, [String; 5])>, NetError> {
        Ok(lock(&self.conn)
            .query(
                "SELECT id, ohl_time(time), user_name, action, target, detail FROM audit
                 WHERE id > $1 ORDER BY id LIMIT $2",
                &[&after, &limit],
            )?
            .into_iter()
            .map(|r| (r.get(0), [r.get(1), r.get(2), r.get(3), r.get(4), r.get(5)]))
            .collect())
    }

    /// A page of the audit, newest first (2026-11-28, OHL-Server's Audit
    /// page): the rows matching `q` older than row `before` (none: from
    /// the newest), at most `limit`; and how many match in all. Reads
    /// only (the audit stays append-only).
    pub fn audit_page(
        &self,
        q: &AuditQuery,
        before: Option<i64>,
        limit: i64,
    ) -> Result<(Vec<(i64, [String; 5])>, i64), NetError> {
        let from = audit_time(&q.from, false)?;
        let until = audit_time(&q.until, true)?;
        const WHERE: &str = "($1 = '' OR user_name = $1)
              AND ($2 = '' OR left(action, length($2)) = $2)
              AND ($3 = '' OR strpos(lower(target), lower($3)) > 0)
              AND ($4 = '' OR strpos(lower(detail), lower($4)) > 0)
              AND ($5::text IS NULL OR time >= $5::text::timestamptz)
              AND ($6::text IS NULL OR time <= $6::text::timestamptz)";
        let mut c = lock(&self.conn);
        let total: i64 = c
            .client
            .query_one(
                &format!("SELECT count(*) FROM audit WHERE {WHERE}"),
                &[&q.user, &q.action, &q.target, &q.text, &from, &until],
            )?
            .get(0);
        let rows = c
            .client
            .query(
                &format!(
                    "SELECT id, ohl_time(time), user_name, action, target, detail FROM audit
                     WHERE {WHERE} AND ($7::bigint IS NULL OR id < $7)
                     ORDER BY id DESC LIMIT $8"
                ),
                &[
                    &q.user, &q.action, &q.target, &q.text, &from, &until, &before, &limit,
                ],
            )?
            .into_iter()
            .map(|r| (r.get(0), [r.get(1), r.get(2), r.get(3), r.get(4), r.get(5)]))
            .collect();
        Ok((rows, total))
    }
}

/// Which audit rows [`Network::audit_page`] reads (every field: empty, no
/// constraint).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditQuery {
    /// The user, exactly.
    pub user: String,
    /// The action's start (`admin.`, `library.`, `refused`).
    pub action: String,
    /// Text in the target (any case).
    pub target: String,
    /// Text in the detail (any case).
    pub text: String,
    /// From (UTC, `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`).
    pub from: String,
    /// Until (UTC, as `from`; a day alone: to its end).
    pub until: String,
}

/// A filter's time (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`, UTC) as
/// PostgreSQL reads it; a day alone as `until`: the next day's start.
fn audit_time(t: &str, until: bool) -> Result<Option<String>, NetError> {
    let t = t.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let b = t.as_bytes();
    let digits = |r: std::ops::Range<usize>| r.into_iter().all(|i| b[i].is_ascii_digit());
    let day = b.len() >= 10 && digits(0..4) && b[4] == b'-' && digits(5..7) && b[7] == b'-'
        && digits(8..10);
    let ok = day
        && (b.len() == 10
            || (b.len() == 16 && b[10] == b' ' && digits(11..13) && b[13] == b':'
                && digits(14..16)));
    if !ok {
        return Err(NetError::Refused(format!(
            "\"{t}\": a time as YYYY-MM-DD or YYYY-MM-DD HH:MM (UTC)"
        )));
    }
    Ok(Some(if b.len() == 10 {
        if until {
            format!("{t} 23:59:59.999999+00")
        } else {
            format!("{t} 00:00+00")
        }
    } else {
        format!("{t}+00")
    }))
}

/// Whether `actor` holds every permission of `perms` by grants whose
/// scopes hold `scope` (no one gives more than they hold).
fn mine_holds(data: &RightsData, actor: &str, scope: &Scope, perms: Perms) -> bool {
    let mut held = Perms::NONE;
    for a in data.assignments_of(actor) {
        if a.deny || !a.scope.contains(scope) {
            continue;
        }
        if let Some((p, _)) = a.role.as_ref().and_then(|r| data.roles.get(r)) {
            held = held.union(*p);
        }
    }
    data.active(actor) && held.covers(perms)
}

/// Scopes resolved from the database: a region's lines by PostGIS; a
/// route's part by opening its model (the command line; the server
/// resolves from the models it holds).
pub struct DbResolve<'a>(pub &'a Network);

impl Resolve for DbResolve<'_> {
    fn route_part(
        &self,
        line: &str,
        route: &str,
        from: &str,
        to: &str,
    ) -> Option<HashSet<(Kind, String)>> {
        let store = self.0.store(line).ok()?;
        let mut s = ohl_session::Session::open_on(Box::new(store), line).ok()?;
        let e = s
            .pick_extent(&[ohl_session::working_set::Pick::Structures {
                route: route.into(),
                from: from.into(),
                to: to.into(),
            }])
            .ok()?;
        Some(e.wanted.into_iter().collect())
    }

    fn region_lines(&self, region: &[[f64; 2]]) -> BTreeSet<String> {
        lock(&self.0.conn)
            .query(
                "SELECT DISTINCT line_id FROM spatial
                 WHERE branch = 'main' AND ST_Intersects(geom, ST_GeomFromText($1))",
                &[&wkt(region)],
            )
            .map(|rows| rows.into_iter().map(|r| r.get(0)).collect())
            .unwrap_or_default()
    }
}

/// The rights in words: users, groups, roles, assignments (the command
/// line's listing).
pub fn describe(d: &RightsData) -> String {
    let mut out = String::new();
    out.push_str(&format!("rights revision {}\n", d.rev));
    out.push_str("roles:\n");
    for (name, (p, shipped)) in &d.roles {
        out.push_str(&format!(
            "  {name}{}: {}\n",
            if *shipped { " (shipped)" } else { "" },
            p.describe()
        ));
    }
    out.push_str("users:\n");
    for (name, active) in &d.users {
        let groups: Vec<&str> = d.groups_of(name).into_iter().collect();
        out.push_str(&format!(
            "  {name}{}{}\n",
            if *active { "" } else { " (disabled)" },
            if groups.is_empty() {
                String::new()
            } else {
                format!(" in {}", groups.join(", "))
            }
        ));
    }
    out.push_str("groups:\n");
    for (g, m) in &d.groups {
        out.push_str(&format!(
            "  {g}: {}\n",
            m.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    out.push_str("assignments:\n");
    for a in &d.assignments {
        out.push_str(&format!("  {}\n", a.describe()));
    }
    out
}
