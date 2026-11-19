// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A throwaway PostgreSQL cluster (feature `testing`), for the tests of
//! the crates working on a network (the server's): made in a folder of its
//! own (`initdb`), started on a free port of 127.0.0.1 only, without a
//! Unix socket (`pg_ctl`), and stopped and deleted when dropped — also when
//! a test fails. Never a service.
//!
//! The binaries: `OHL_PG_BIN`, else Homebrew's `postgresql@17`, else
//! `pg_config --bindir`. Without them [`Cluster::start`] says so and gives
//! none: the tests pass without running (`OHL_REQUIRE_PG_TESTS=1` makes
//! that a failure).

use std::path::{Path, PathBuf};
use std::process::Command;

/// A running throwaway cluster.
pub struct Cluster {
    bin: PathBuf,
    dir: PathBuf,
    port: u16,
}

fn bin_dir() -> Option<PathBuf> {
    let has = |d: &Path| d.join("initdb").is_file() && d.join("pg_ctl").is_file();
    if let Some(d) = std::env::var_os("OHL_PG_BIN").map(PathBuf::from) {
        return has(&d).then_some(d);
    }
    let brew = PathBuf::from("/opt/homebrew/opt/postgresql@17/bin");
    if has(&brew) {
        return Some(brew);
    }
    let out = Command::new("pg_config").arg("--bindir").output().ok()?;
    let d = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    has(&d).then_some(d)
}

impl Cluster {
    /// Starts a cluster named `name` (its folder in the temporary
    /// directory); `None` (said on stderr) without PostgreSQL's binaries.
    ///
    /// # Panics
    ///
    /// When the binaries are there but the cluster cannot be made or
    /// started (a test should fail then), or without them when
    /// `OHL_REQUIRE_PG_TESTS` is set.
    pub fn start(name: &str) -> Option<Self> {
        let Some(bin) = bin_dir() else {
            let message = "PostgreSQL's binaries not found (initdb, pg_ctl; set OHL_PG_BIN): \
                           the tests on a network database are skipped";
            assert!(
                std::env::var_os("OHL_REQUIRE_PG_TESTS").is_none(),
                "{message}"
            );
            eprintln!("{message}");
            return None;
        };
        let dir = std::env::temp_dir().join(format!("ohl-pg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("cluster folder");
        let status = Command::new(bin.join("initdb"))
            .env("LC_ALL", "en_US.UTF-8")
            .args(["-D"])
            .arg(dir.join("data"))
            .args(["-U", "ohl", "--auth=trust", "--no-locale", "-E", "UTF8"])
            .stdout(std::process::Stdio::null())
            .status()
            .expect("initdb");
        assert!(status.success(), "initdb failed");
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free port")
            .port();
        let cluster = Self { bin, dir, port };
        // Throwaway: no fsync (a test database is never kept).
        let options = format!(
            "-p {port} -c listen_addresses=127.0.0.1 -c unix_socket_directories='' -c fsync=off \
             -c synchronous_commit=off -c full_page_writes=off"
        );
        let status = Command::new(cluster.bin.join("pg_ctl"))
            .env("LC_ALL", "en_US.UTF-8")
            .arg("-D")
            .arg(cluster.dir.join("data"))
            .arg("-l")
            .arg(cluster.dir.join("log"))
            .args(["-w", "-t", "60", "-o", &options, "start"])
            .stdout(std::process::Stdio::null())
            .status()
            .expect("pg_ctl start");
        if !status.success() {
            let log = std::fs::read_to_string(cluster.dir.join("log")).unwrap_or_default();
            panic!("PostgreSQL did not start:\n{log}");
        }
        Some(cluster)
    }

    /// The connection string of database `db`.
    pub fn url(&self, db: &str) -> String {
        format!(
            "host=127.0.0.1 port={} user=ohl dbname={db} connect_timeout=10",
            self.port
        )
    }

    /// Makes database `db` (empty).
    ///
    /// # Panics
    ///
    /// When it cannot be made.
    pub fn create_db(&self, db: &str) {
        let mut c = postgres::Client::connect(&self.url("postgres"), postgres::NoTls)
            .expect("the cluster's postgres database");
        c.batch_execute(&format!("CREATE DATABASE \"{db}\" TEMPLATE template0"))
            .expect("CREATE DATABASE");
    }

    /// A folder for the test's files, deleted with the cluster.
    ///
    /// # Panics
    ///
    /// When it cannot be made.
    pub fn scratch(&self) -> PathBuf {
        let d = self.dir.join("files");
        std::fs::create_dir_all(&d).expect("scratch folder");
        d
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        let _ = Command::new(self.bin.join("pg_ctl"))
            .env("LC_ALL", "en_US.UTF-8")
            .arg("-D")
            .arg(self.dir.join("data"))
            .args(["-w", "-t", "60", "-m", "fast", "stop"])
            .stdout(std::process::Stdio::null())
            .status();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
