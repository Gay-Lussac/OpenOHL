// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Host mode: the network's server started inside the app (a small office,
//! a test) against a PostgreSQL the admin names, with the settings of `ohl
//! network serve` (the connection string, its password file, TLS). The
//! app then joins its own server like any admin app, through the protocol.
//! The database is touched here only as the server program touches it at
//! its start: its schema looked at (a migration is the server's, refused
//! without a recent backup), and, when asked, the first network admin
//! (only while the network has none, as `ohl network rights bootstrap`).

use std::path::PathBuf;
use std::sync::mpsc;

use ohl_server::network::NetOptions;
use ohl_server::server::{Options, Server};

use crate::conn::JoinHow;

/// The host mode's settings (nothing of it is saved: a connection string
/// may hold a password).
#[derive(Debug, Clone)]
pub struct HostForm {
    /// The database's connection string (`host=… dbname=…` or
    /// `postgresql://…`).
    pub database: String,
    /// A file holding the database's password (empty: none, or in the
    /// string).
    pub password_file: String,
    /// TLS to the database: empty (the string's, else PostgreSQL's
    /// default), `disable`, `require`, `verify-ca`, `verify-full`.
    pub tls: String,
    /// The database's CA (PEM; empty: the system's).
    pub ca_file: String,
    /// Address and port to listen on.
    pub bind: String,
    /// Host name or address in the invitations (empty: this computer's on
    /// the local network).
    pub advertise: String,
    /// The file keeping the personal invitations' key (empty: a random key
    /// for this run — invitations change at the next start).
    pub keys: String,
    /// The folder of the backups made from the app (empty: none).
    pub backup_dir: String,
    /// The admin who signs in (a network admin of the database).
    pub user: String,
    /// Make `user` the first network admin (only while the network has
    /// none).
    pub bootstrap: bool,
}

impl Default for HostForm {
    fn default() -> Self {
        Self {
            database: String::new(),
            password_file: String::new(),
            tls: String::new(),
            ca_file: String::new(),
            bind: "0.0.0.0:4433".into(),
            advertise: String::new(),
            keys: String::new(),
            backup_dir: String::new(),
            user: String::new(),
            bootstrap: false,
        }
    }
}

fn some(s: &str) -> Option<&str> {
    let s = s.trim();
    (!s.is_empty()).then_some(s)
}

impl HostForm {
    /// The connection string with its TLS and password (never shown).
    pub fn url(&self) -> Result<String, String> {
        let db = some(&self.database).ok_or("the database's connection string")?;
        let url = ohl_store_pg::connect::with_tls(
            db,
            some(&self.tls),
            some(&self.ca_file).map(std::path::Path::new),
        );
        match some(&self.password_file) {
            Some(f) => ohl_store_pg::connect::with_password_file(&url, std::path::Path::new(f))
                .map_err(|e| e.to_string()),
            None => Ok(url),
        }
    }
}

/// What the database holds, looked at before the server starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// The network's schema (0: an empty database).
    pub schema: i32,
    /// This program's.
    pub program: i32,
    /// The network admins (none on a new network).
    pub admins: Vec<String>,
}

impl Look {
    /// In words.
    pub fn describe(&self) -> String {
        let schema = if self.schema == 0 {
            "an empty database (the network's schema is made at the start)".to_owned()
        } else if self.schema < self.program {
            format!(
                "network schema {}, this program's {}: migrations pending — the server migrates \
                 at its start, refused without a backup younger than the network's \
                 backup_max_age_hours (default 24 h)",
                self.schema, self.program
            )
        } else {
            format!("network schema {} (this program's)", self.schema)
        };
        let admins = if self.admins.is_empty() {
            "no network admin yet: tick \"first network admin\"".to_owned()
        } else {
            format!("network admins: {}", self.admins.join(", "))
        };
        format!("{schema}; {admins}")
    }
}

/// Looks at the database (its schema, its network admins).
pub fn look(form: &HostForm) -> Result<Look, String> {
    let url = form.url()?;
    let net = ohl_store_pg::Network::connect_as_is(&url).map_err(|e| e.to_string())?;
    let schema = net.schema_as_is().map_err(|e| e.to_string())?;
    let admins = if schema >= ohl_store_pg::schema::SCHEMA {
        net.rights()
            .map(|d| d.network_admins())
            .map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    Ok(Look {
        schema,
        program: ohl_store_pg::schema::SCHEMA,
        admins,
    })
}

/// The server hosted by the app.
pub struct Hosted {
    /// The server (stopped when dropped).
    pub server: Server,
}

/// A server starting (on its thread: every line is read first).
pub struct Starting {
    rx: mpsc::Receiver<Result<(Hosted, JoinHow), String>>,
}

impl Starting {
    /// Starts the server of `form`.
    pub fn start(form: HostForm) -> Starting {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("ohl-server-app-host".into())
            .spawn(move || {
                let _ = tx.send(start(&form));
            })
            .expect("a thread");
        Starting { rx }
    }

    /// The server once started, or why not.
    pub fn poll(&self) -> Option<Result<(Hosted, JoinHow), String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the start stopped".into())),
        }
    }
}

/// Starts the server of `form` and makes the admin's invitation to it.
pub fn start(form: &HostForm) -> Result<(Hosted, JoinHow), String> {
    let user = some(&form.user).ok_or("the admin who signs in")?.to_owned();
    let url = form.url()?;
    if form.bootstrap {
        // As `ohl network rights bootstrap` (the rights' own function:
        // refused once the network has an admin; in the audit).
        let net = ohl_store_pg::Network::connect(&url).map_err(|e| e.to_string())?;
        net.rights_command(
            &user,
            &ohl_session::rights::RightsCommand::Bootstrap { user: user.clone() },
        )
        .map_err(|e| e.to_string())?;
    }
    let bind = form
        .bind
        .trim()
        .parse()
        .map_err(|_| format!("\"{}\": an address and port to listen on", form.bind))?;
    let server = Server::start_network(
        Options {
            bind,
            advertise: some(&form.advertise).map(str::to_owned),
            code: None,
            keys: some(&form.keys).map(PathBuf::from),
            newcomers: None,
            web: None,
            ..Options::default()
        },
        NetOptions {
            database: url,
            backup_dir: some(&form.backup_dir).map(PathBuf::from),
            invite_days: Some(ohl_server::network::DEFAULT_INVITE_DAYS),
            ..NetOptions::default()
        },
    )?;
    let mut invitation = server.invitation_valid(&user, None)?;
    // The app joins its own server on this computer.
    invitation.address = format!("127.0.0.1:{}", server.local_addr.port());
    Ok((
        Hosted { server },
        JoinHow {
            invitation,
            user,
            id_token: None,
        },
    ))
}
