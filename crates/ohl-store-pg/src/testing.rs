// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Throwaway PostgreSQL for the tests (feature `testing`; the store's own
//! integration tests include this file too): **one cluster per test
//! program**, shared by its tests — each test in databases of its own,
//! dropped at its end — made in a folder of its own (`initdb`), started on
//! a free port of 127.0.0.1 only, without a Unix socket (`pg_ctl`), and
//! stopped and deleted when the program's last test using it ends — also
//! when a test fails. Never a service.
//!
//! nextest runs every test in a process of its own, so the cluster is
//! shared between processes, by files in `ohl-pg-tests/` of the temporary
//! directory (O46, 2026-11-19):
//!
//! - `KEY.lock`: the key's lock (a lock on the whole file, `flock`), held
//!   while a test joins, makes, leaves or stops the cluster. The key is
//!   the test program's path and PostgreSQL's binaries: two programs, two
//!   clusters; two runs of the same program may share one (their tests
//!   are in databases of their own all the same). The file stays.
//! - `KEY/`: the cluster (`data/`, `log`, `port`) and `users.lock`, on
//!   which every test using the cluster holds a shared lock as long as it
//!   uses it. A test leaving takes the key's lock, lets its shared lock go
//!   and tries the exclusive one: got, no test uses the cluster any more —
//!   it is stopped and deleted. The operating system lets a lock go with
//!   its process, so a test killed (a timeout) cannot keep a cluster
//!   alive: the next test of any program finds it unused (no shared lock,
//!   the key free) and stops and deletes it.
//! - The port: a free one asked of the system, and the cluster started on
//!   it; another program may take the same port between (two runs side by
//!   side: 2026-11-19, a test connected to the other run's cluster), so a
//!   start refused for its port is made again on another one, and every
//!   cluster joined or started is **checked to be this one**: its
//!   `data_directory` is this folder (else stopped, never used).
//!
//! The binaries: `OHL_PG_BIN`, else Homebrew's `postgresql@17`, else
//! `pg_config --bindir`. Without them [`Cluster::start`] says so and gives
//! none: the tests pass without running (`OHL_REQUIRE_PG_TESTS=1` makes
//! that a failure).

#![allow(dead_code)]

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How long a test waits for the key's lock (another test making or
/// stopping the cluster: a second or two) before failing.
const LOCK_WAIT: Duration = Duration::from_secs(180);

/// Ports tried before a start fails.
const PORT_TRIES: usize = 20;

/// One test's part of the program's cluster: its databases (named with
/// the test's prefix) and its folder of files, both deleted when dropped,
/// and its hold on the cluster (the cluster stopped and deleted with the
/// last).
pub struct Cluster {
    bin: PathBuf,
    /// The cluster's folder.
    dir: PathBuf,
    /// The key's lock file.
    key_lock: PathBuf,
    port: u16,
    /// This test's prefix of its databases.
    prefix: String,
    /// The databases made (full names).
    made: std::sync::Mutex<Vec<String>>,
    /// This test's files.
    files: PathBuf,
    /// The shared lock on `users.lock` (the hold on the cluster).
    hold: Option<File>,
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

/// FNV-1a, 64 bits (a stable name for a key; not for security).
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The folder of the shared clusters and their locks.
fn base() -> PathBuf {
    std::env::temp_dir().join("ohl-pg-tests")
}

/// Opens (makes) a lock file.
fn lock_file(path: &Path) -> File {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The key's lock, waited for at most [`LOCK_WAIT`].
fn lock_key(path: &Path) -> File {
    let f = lock_file(path);
    let until = Instant::now() + LOCK_WAIT;
    loop {
        match f.try_lock() {
            Ok(()) => return f,
            Err(TryLockError::WouldBlock) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(TryLockError::WouldBlock) => panic!(
                "{}: held for more than {LOCK_WAIT:?} by another test (a cluster being made or \
                 stopped)",
                path.display()
            ),
            Err(TryLockError::Error(e)) => panic!("{}: {e}", path.display()),
        }
    }
}

/// `pg_ctl` on the cluster in `dir`.
fn pg_ctl(bin: &Path, dir: &Path) -> Command {
    let mut c = Command::new(bin.join("pg_ctl"));
    c.env("LC_ALL", "en_US.UTF-8")
        .arg("-D")
        .arg(dir.join("data"))
        .stdout(Stdio::null());
    c
}

/// Stops the cluster in `dir` (if it runs) and deletes its folder.
fn stop_and_delete(bin: &Path, dir: &Path) {
    if dir.join("data/postmaster.pid").exists() {
        let _ = pg_ctl(bin, dir)
            .args(["-w", "-t", "60", "-m", "fast", "stop"])
            .status();
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Whether the server on `port` is the cluster in `dir`: its
/// `data_directory` is that folder. A connection refused or timed out is
/// tried again twice (a machine busy with other tests).
fn is_this_cluster(port: u16, dir: &Path) -> Result<(), String> {
    let mut why = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(500));
        }
        match server_is(port, dir) {
            Ok(true) => return Ok(()),
            Ok(false) => break,
            Err(e) => why = e,
        }
    }
    if why.is_empty() {
        why = format!("port {port} is another cluster's, not {}", dir.display());
    }
    Err(why)
}

/// Whether the server on `port` has `dir`'s data directory (an error: no
/// answer).
fn server_is(port: u16, dir: &Path) -> Result<bool, String> {
    let url = format!("host=127.0.0.1 port={port} user=ohl dbname=postgres connect_timeout=10");
    let mut c = postgres::Client::connect(&url, postgres::NoTls).map_err(|e| e.to_string())?;
    let row = c
        .query_one("SHOW data_directory", &[])
        .map_err(|e| e.to_string())?;
    let theirs: String = row.get(0);
    Ok(
        match (
            std::fs::canonicalize(&theirs),
            std::fs::canonicalize(dir.join("data")),
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        },
    )
}

/// Makes the cluster in `dir` and starts it on a free port, another one
/// when its port was taken meanwhile; checked to be this cluster. The
/// port.
fn make(bin: &Path, dir: &Path) -> u16 {
    let status = Command::new(bin.join("initdb"))
        .env("LC_ALL", "en_US.UTF-8")
        .arg("-D")
        .arg(dir.join("data"))
        .args(["-U", "ohl", "--auth=trust", "--no-locale", "-E", "UTF8"])
        .stdout(Stdio::null())
        .status()
        .expect("initdb");
    assert!(status.success(), "initdb failed");
    let log = dir.join("log");
    let mut tried = Vec::new();
    for _ in 0..PORT_TRIES {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free port")
            .port();
        // Throwaway: no fsync (a test database is never kept). Several
        // tests at once, each with a few connections.
        let options = format!(
            "-p {port} -c listen_addresses=127.0.0.1 -c unix_socket_directories='' -c fsync=off \
             -c synchronous_commit=off -c full_page_writes=off -c max_connections=200"
        );
        let _ = std::fs::remove_file(&log);
        let started = pg_ctl(bin, dir)
            .arg("-l")
            .arg(&log)
            .args(["-w", "-t", "60", "-o", &options, "start"])
            .status()
            .expect("pg_ctl start");
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if !started.success() {
            // Its port taken meanwhile: another port.
            if text.contains("could not create any TCP/IP sockets")
                || text.contains("Address already in use")
            {
                tried.push(port);
                continue;
            }
            panic!("PostgreSQL did not start:\n{text}");
        }
        match is_this_cluster(port, dir) {
            Ok(()) => return port,
            Err(why) => {
                let _ = pg_ctl(bin, dir)
                    .args(["-w", "-t", "60", "-m", "fast", "stop"])
                    .status();
                tried.push(port);
                eprintln!("PostgreSQL test cluster: {why}; another port");
            }
        }
    }
    panic!("PostgreSQL did not start: ports {tried:?} taken by others meanwhile");
}

/// Stops and deletes the clusters no test uses any more (their tests
/// killed), other than `keep`.
fn sweep(bin: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(base()) else {
        return;
    };
    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() || dir == keep {
            continue;
        }
        let key = dir.with_extension("lock");
        let k = lock_file(&key);
        if k.try_lock().is_err() {
            continue;
        }
        let users = lock_file(&dir.join("users.lock"));
        if users.try_lock().is_ok() {
            stop_and_delete(bin, &dir);
        }
    }
}

/// Counts the tests of this process (threads under `cargo test`).
static NEXT: AtomicUsize = AtomicUsize::new(0);

impl Cluster {
    /// Joins this test program's cluster (made and started when none
    /// runs) as test `name`; `None` (said on stderr) without PostgreSQL's
    /// binaries.
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
        let base = base();
        std::fs::create_dir_all(&base).expect("the clusters' folder");
        let exe = std::env::current_exe().expect("the test program");
        let key = format!(
            "{:016x}",
            fnv(&format!("{}\n{}", exe.display(), bin.display()))
        );
        let dir = base.join(&key);
        let key_lock = base.join(format!("{key}.lock"));
        sweep(&bin, &dir);

        let k = lock_key(&key_lock);
        let users = dir.join("users.lock");
        let mut port = None;
        if let Ok(text) = std::fs::read_to_string(dir.join("port")) {
            let p: u16 = text.trim().parse().unwrap_or(0);
            match is_this_cluster(p, &dir) {
                Ok(()) => port = Some(p),
                Err(why) => {
                    // Not running any more (or its port another's): made
                    // again when no test uses it, else a failure.
                    let alone = lock_file(&users).try_lock().is_ok();
                    assert!(
                        alone,
                        "the shared PostgreSQL test cluster {} is gone while tests use it: {why}",
                        dir.display()
                    );
                    stop_and_delete(&bin, &dir);
                }
            }
        } else if dir.exists() {
            // Left half made (a test killed while making it).
            stop_and_delete(&bin, &dir);
        }
        if port.is_none() {
            std::fs::create_dir_all(&dir).expect("cluster folder");
        }
        // Held before the cluster is made: never swept while made.
        let hold = lock_file(&users);
        hold.lock_shared().expect("the cluster's users lock");
        let port = port.unwrap_or_else(|| {
            let p = make(&bin, &dir);
            std::fs::write(dir.join("port"), p.to_string()).expect("the cluster's port");
            p
        });
        drop(k);

        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let clean: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .take(20)
            .collect();
        let prefix = format!("t{}_{n}_{clean}", std::process::id());
        let files = dir.join(format!("files-{prefix}"));
        let cluster = Self {
            bin,
            dir,
            key_lock,
            port,
            prefix,
            made: std::sync::Mutex::new(Vec::new()),
            files,
            hold: Some(hold),
        };
        // Databases of an earlier test of the same prefix (a process of
        // the same number, killed): gone.
        let mut c = cluster.admin();
        let old: Vec<String> = c
            .query(
                "SELECT datname FROM pg_database WHERE starts_with(datname, $1)",
                &[&format!("{}_", cluster.prefix)],
            )
            .expect("the cluster's databases")
            .iter()
            .map(|r| r.get(0))
            .collect();
        for db in old {
            let _ = c.batch_execute(&format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"));
        }
        let _ = std::fs::remove_dir_all(&cluster.files);
        Some(cluster)
    }

    /// The full name of this test's database `db`.
    pub fn db_name(&self, db: &str) -> String {
        let name = format!("{}_{db}", self.prefix);
        assert!(name.len() <= 63, "database name too long: {name}");
        name
    }

    /// The connection string of this test's database `db`.
    pub fn url(&self, db: &str) -> String {
        format!(
            "host=127.0.0.1 port={} user=ohl dbname={} connect_timeout=10",
            self.port,
            self.db_name(db)
        )
    }

    /// A connection to the cluster's maintenance database (`postgres`).
    ///
    /// # Panics
    ///
    /// When it cannot connect.
    pub fn admin(&self) -> postgres::Client {
        let url = format!(
            "host=127.0.0.1 port={} user=ohl dbname=postgres connect_timeout=10",
            self.port
        );
        postgres::Client::connect(&url, postgres::NoTls).expect("the cluster's postgres database")
    }

    /// Makes this test's database `db` (empty).
    ///
    /// # Panics
    ///
    /// When it cannot be made.
    pub fn create_db(&self, db: &str) {
        self.create_db_from(db, None);
    }

    /// Makes this test's database `db` as a copy of this test's database
    /// `template` (none: empty).
    ///
    /// # Panics
    ///
    /// When it cannot be made.
    pub fn create_db_from(&self, db: &str, template: Option<&str>) {
        let name = self.db_name(db);
        let template = template.map_or_else(|| "template0".to_owned(), |t| self.db_name(t));
        self.admin()
            .batch_execute(&format!(
                "CREATE DATABASE \"{name}\" TEMPLATE \"{template}\""
            ))
            .expect("CREATE DATABASE");
        self.made.lock().expect("databases made").push(name);
    }

    /// A folder for the test's files, deleted at its end.
    ///
    /// # Panics
    ///
    /// When it cannot be made.
    pub fn scratch(&self) -> PathBuf {
        std::fs::create_dir_all(&self.files).expect("scratch folder");
        self.files.clone()
    }

    /// Waits until no connection holds an advisory lock in this test's
    /// database `db` (at most 10 s, else a failure). A connection closed
    /// by the client lets its locks go when its server process has ended,
    /// a moment later: a writer dropped and another connecting at once
    /// may find the lock still held (2026-11-19: 1 run in 144 under load,
    /// `schema_and_the_writers_lock`).
    ///
    /// # Panics
    ///
    /// When a lock is still held after 10 s.
    pub fn wait_for_locks_gone(&self, db: &str) {
        let mut c = self.admin();
        let name = self.db_name(db);
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let held: i64 = c
                .query_one(
                    "SELECT count(*) FROM pg_locks l JOIN pg_database d ON d.oid = l.database \
                     WHERE l.locktype = 'advisory' AND d.datname = $1",
                    &[&name],
                )
                .expect("the cluster's locks")
                .get(0);
            if held == 0 {
                return;
            }
            assert!(
                Instant::now() < until,
                "{name}: an advisory lock still held 10 s after its connection was closed"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The port the cluster listens on (127.0.0.1).
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        // This test's databases and files.
        let made = std::mem::take(&mut *self.made.lock().unwrap_or_else(|e| e.into_inner()));
        if !made.is_empty() {
            let url = format!(
                "host=127.0.0.1 port={} user=ohl dbname=postgres connect_timeout=10",
                self.port
            );
            if let Ok(mut c) = postgres::Client::connect(&url, postgres::NoTls) {
                // The copies before their template.
                for db in made.iter().rev() {
                    let _ =
                        c.batch_execute(&format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"));
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.files);
        // The last test out stops and deletes the cluster.
        let k = lock_key(&self.key_lock);
        drop(self.hold.take());
        let users = lock_file(&self.dir.join("users.lock"));
        if users.try_lock().is_ok() {
            stop_and_delete(&self.bin, &self.dir);
        }
        drop(users);
        drop(k);
    }
}
