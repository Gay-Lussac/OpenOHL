// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Phase 13 on a throwaway cluster (`common`): a backup and the restore
//! drill (every model and the network's own tables the same, byte for
//! byte, in the database restored), the restore's refusals; schema
//! migrations refused without a recent backup, an admin's
//! `--i-have-a-backup` in the audit; the network's settings; TLS to the
//! database (`require`, `verify-ca`, `verify-full` with the owner's CA).

mod common;

use std::path::{Path, PathBuf};

use common::Cluster;
use ohl_session::{Command, Role, Session};
use ohl_store_pg::backup::{self, Manifest};
use ohl_store_pg::import::{ImportOptions, import};
use ohl_store_pg::{NetError, Network};

fn example() -> ohl_line::Line {
    ohl_line::Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

/// A network of two models (the example line twice, the second's ids
/// renamed), edited, committed, branched; a network admin.
fn network(cluster: &Cluster, db: &str) -> String {
    cluster.create_db(db);
    let url = cluster.url(db);
    let dir = cluster.scratch();
    let file = dir.join(format!("{db}.ohl"));
    {
        let mut s = Session::open(&file).unwrap();
        s.import(&example(), "ana").unwrap();
        s.set_user("ana", "ben", Role::Designer).unwrap();
    }
    let net = Network::connect_writer(&url).unwrap();
    for code in ["A", "B"] {
        let mut o = ImportOptions::new(code, "ana");
        o.revision = true;
        import(&net, &file, &o).unwrap();
    }
    use ohl_store_pg::rights::RightsCommand as R;
    net.rights_command("ana", &R::Bootstrap { user: "ana".into() })
        .unwrap();
    let mut s = Session::open_on(Box::new(net.store("A").unwrap()), "A").unwrap();
    let id = s.line().structures[2].id.clone();
    let p = s.line().structure(&id).unwrap().position;
    s.apply(
        "ana",
        Command::MoveStructure {
            id: id.clone(),
            position: [p[0] + 1.5, p[1], p[2]],
        },
    )
    .unwrap();
    s.commit("ana", "moved").unwrap();
    s.create_branch("ana", "variant", Some("main")).unwrap();
    drop(s);
    drop(net);
    cluster.wait_for_locks_gone(db);
    url
}

/// Every model's rows and the network's own, as the drill compares them.
fn dumps(url: &str) -> Vec<(String, Vec<u8>)> {
    let net = Network::connect_as_is(url).unwrap();
    let mut out = Vec::new();
    for l in net.lines().unwrap() {
        let mut d = Vec::new();
        backup::model_dump(&net, &l.id, &mut d).unwrap();
        out.push((l.id, d));
    }
    let mut d = Vec::new();
    backup::network_dump(&net, &mut d).unwrap();
    out.push(("*".into(), d));
    out
}

#[test]
fn backup_and_the_restore_drill() {
    let Some(cluster) = Cluster::start("backup") else {
        return;
    };
    let url = network(&cluster, "net");
    let before = dumps(&url);
    let dir = cluster.scratch().join("backup-1");
    // Network admins only.
    Network::connect(&url)
        .unwrap()
        .rights_command(
            "ana",
            &ohl_store_pg::rights::RightsCommand::AddUser {
                name: "ben".into(),
                display: None,
            },
        )
        .unwrap();
    let e = backup::backup(&url, "ben", &dir).unwrap_err().to_string();
    assert!(e.contains("network admins only"), "{e}");
    let before_ben = dumps(&url);
    let r = backup::backup(&url, "ana", &dir).unwrap();
    let m = &r.manifest;
    assert_eq!(m.schema, ohl_store_pg::schema::SCHEMA);
    assert_eq!(m.models.len(), 2);
    assert!(m.bytes > 0 && m.sha256.len() == 64);
    assert!(dir.join(backup::DUMP).is_file() && dir.join(backup::MANIFEST).is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    // The backup recorded (and in the audit), after the snapshot.
    let net = Network::connect(&url).unwrap();
    let last = net.last_backup().unwrap().unwrap();
    assert_eq!(last["sha256"], m.sha256.as_str());
    assert!(
        net.audit_rows()
            .unwrap()
            .iter()
            .any(|a| a[2] == "backup" && a[1] == "ana")
    );
    // A folder used already: refused.
    assert!(backup::backup(&url, "ana", &dir).is_err());

    // Restored into an empty database: the drill compares every model's
    // and the network's rows with the manifest's hashes, and here byte
    // for byte with the database backed up (as it was in the snapshot).
    cluster.create_db("copy");
    let copy = cluster.url("copy");
    let restored = backup::restore(&copy, "ana", &dir).unwrap();
    assert_eq!(restored.models, 2);
    let after = dumps(&copy);
    // What the snapshot held: the database before the backup recorded
    // itself (the refusal of ben's backup is in the audit already).
    // Each model byte for byte (the network's own tables were checked
    // against the manifest by the restore, before it wrote its audit).
    assert_eq!(before_ben.len(), after.len());
    for ((a, x), (b, y)) in before_ben.iter().zip(&after) {
        assert_eq!(a, b);
        if a != "*" {
            assert!(!x.is_empty() && x == y, "{a}: the restored rows differ");
        }
    }
    assert_eq!(r.manifest.network.rows, restored.manifest.network.rows);
    assert_ne!(before, before_ben, "the refusal is in the audit");
    // The restored network opens and works: a session on a model.
    let restored_net = Network::connect_writer(&copy).unwrap();
    let s = Session::open_on(Box::new(restored_net.store("A").unwrap()), "A").unwrap();
    assert_eq!(s.line().structures.len(), example().structures.len());
    drop(s);
    assert!(
        restored_net
            .audit_rows()
            .unwrap()
            .iter()
            .any(|a| a[2] == "restore")
    );
    drop(restored_net);

    // Refused: a database that is not empty, a dump changed since, a
    // backup of another schema, a manifest naming another file.
    let e = backup::restore(&copy, "ana", &dir).unwrap_err().to_string();
    assert!(e.contains("not empty"), "{e}");
    let tampered = cluster.scratch().join("tampered");
    copy_dir(&dir, &tampered);
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(tampered.join(backup::DUMP))
            .unwrap();
        f.write_all(b"x").unwrap();
    }
    cluster.create_db("empty");
    let empty = cluster.url("empty");
    let e = backup::restore(&empty, "ana", &tampered)
        .unwrap_err()
        .to_string();
    assert!(e.contains("changed or damaged"), "{e}");
    let older = cluster.scratch().join("older");
    copy_dir(&dir, &older);
    edit_manifest(&older, |m| m.schema -= 1);
    let e = backup::restore(&empty, "ana", &older)
        .unwrap_err()
        .to_string();
    assert!(e.contains("network schema"), "{e}");
    let elsewhere = cluster.scratch().join("elsewhere");
    copy_dir(&dir, &elsewhere);
    edit_manifest(&elsewhere, |m| m.file = "../../etc/passwd".into());
    let e = backup::restore(&empty, "ana", &elsewhere)
        .unwrap_err()
        .to_string();
    assert!(e.contains("must be network.dump"), "{e}");
    // A restore whose rows differ from the manifest's: the drill fails.
    let wrong = cluster.scratch().join("wrong");
    copy_dir(&dir, &wrong);
    edit_manifest(&wrong, |m| m.models[0].sha256 = "0".repeat(64));
    let e = backup::restore(&empty, "ana", &wrong)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("restore drill failed") && e.contains("line A"),
        "{e}"
    );
}

fn copy_dir(from: &Path, to: &PathBuf) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
    }
}

fn edit_manifest(dir: &Path, f: impl FnOnce(&mut Manifest)) {
    let mut m = backup::read_manifest(dir).unwrap();
    f(&mut m);
    std::fs::write(
        dir.join(backup::MANIFEST),
        serde_json::to_string_pretty(&m).unwrap(),
    )
    .unwrap();
}

/// A database of schema 6 (phase 12) with a model and a network admin.
fn schema_6(cluster: &Cluster, db: &str) -> String {
    cluster.create_db(db);
    let url = cluster.url(db);
    let mut c = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    c.batch_execute(
        "CREATE TABLE net_schema (version integer PRIMARY KEY,
             time timestamptz NOT NULL DEFAULT clock_timestamp(), note text NOT NULL)",
    )
    .unwrap();
    for (version, note, sql) in &ohl_store_pg::schema::MIGRATIONS[..6] {
        c.batch_execute(sql).unwrap();
        c.execute(
            "INSERT INTO net_schema (version, note) VALUES ($1, $2)",
            &[version, note],
        )
        .unwrap();
    }
    c.batch_execute(
        "INSERT INTO line (id, name) VALUES ('A', 'a');
         INSERT INTO net_user (name, display) VALUES ('ana', 'ana');
         INSERT INTO assignment (subject_kind, subject, role, scope_kind, by_user)
             VALUES ('user', 'ana', 'network_admin', 'network', 'ana')",
    )
    .unwrap();
    url
}

/// A database of schema 7 (phase 13) whose branches wrote rows of the
/// spatial index (before 2026-11-20), with a model and a network admin.
fn schema_7_with_branch_rows(cluster: &Cluster, db: &str) -> String {
    cluster.create_db(db);
    let url = cluster.url(db);
    let mut c = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    c.batch_execute(
        "CREATE TABLE net_schema (version integer PRIMARY KEY,
             time timestamptz NOT NULL DEFAULT clock_timestamp(), note text NOT NULL)",
    )
    .unwrap();
    for (version, note, sql) in &ohl_store_pg::schema::MIGRATIONS[..7] {
        c.batch_execute(sql).unwrap();
        c.execute(
            "INSERT INTO net_schema (version, note) VALUES ($1, $2)",
            &[version, note],
        )
        .unwrap();
    }
    c.batch_execute(
        "INSERT INTO line (id, name) VALUES ('A', 'a'), ('B', 'b');
         INSERT INTO net_user (name, display) VALUES ('ana', 'ana');
         INSERT INTO assignment (subject_kind, subject, role, scope_kind, by_user)
             VALUES ('user', 'ana', 'network_admin', 'network', 'ana');
         INSERT INTO spatial (line_id, branch, kind, id, level, tx, ty, x0, y0, x1, y1, geom)
         SELECT l, b, 'structure', 'T' || n, 0, n, 0, n * 100, 0, n * 100 + 1, 1,
                ST_MakeEnvelope(n * 100, 0, n * 100 + 1, 1)
         FROM unnest(ARRAY['A', 'B']) AS l,
              unnest(ARRAY['main', 'bob-1', 'offline-3']) AS b,
              generate_series(1, 50) AS n;",
    )
    .unwrap();
    url
}

/// The rows of the spatial index by branch.
fn spatial_branches(url: &str) -> Vec<(String, i64)> {
    postgres::Client::connect(url, postgres::NoTls)
        .unwrap()
        .query(
            "SELECT branch, count(*) FROM spatial GROUP BY branch ORDER BY branch",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect()
}

/// Schema 8 (2026-11-21): the branches' rows of the spatial index, which
/// nothing reads since 2026-11-20, deleted by the migration — only after
/// a recent backup (phase 13's guard), the main lines' rows kept, in the
/// audit; refused, nothing changes.
#[test]
fn old_branch_rows_of_the_spatial_index_cleaned_up_after_a_backup() {
    let Some(cluster) = Cluster::start("spatial8") else {
        return;
    };
    let url = schema_7_with_branch_rows(&cluster, "old");
    let all = vec![
        ("bob-1".to_owned(), 100),
        ("main".to_owned(), 100),
        ("offline-3".to_owned(), 100),
    ];
    assert_eq!(spatial_branches(&url), all);
    // No backup: refused, the rows as they were.
    let e = Network::connect(&url).unwrap_err().to_string();
    assert!(
        e.contains("network schema 7") && e.contains("needs a backup"),
        "{e}"
    );
    assert_eq!(spatial_branches(&url), all);
    // A backup by this program on schema 7: the migration goes.
    let dir = cluster.scratch().join("old-backup");
    let r = backup::backup(&url, "ana", &dir).unwrap();
    assert_eq!(r.manifest.schema, 7);
    let net = Network::connect(&url).unwrap();
    assert_eq!(net.schema().unwrap(), 8);
    assert_eq!(spatial_branches(&url), vec![("main".to_owned(), 100)]);
    let migrated = net
        .audit_rows()
        .unwrap()
        .into_iter()
        .find(|a| a[2] == "migrate")
        .unwrap();
    assert!(migrated[4].contains("7 brought to 8"), "{migrated:?}");
    // The main lines' index still answers.
    assert_eq!(
        net.objects_in_area([0.0, 0.0, 10_000.0, 10.0]).unwrap(),
        100
    );
    drop(net);
    // --i-have-a-backup: the same, in the audit.
    let url = schema_7_with_branch_rows(&cluster, "old2");
    let (from, to) = Network::connect_as_is(&url)
        .unwrap()
        .migrate_now("ana", true)
        .unwrap();
    assert_eq!((from, to), (7, 8));
    assert_eq!(spatial_branches(&url), vec![("main".to_owned(), 100)]);
}

#[test]
fn migrations_wait_for_a_recent_backup() {
    let Some(cluster) = Cluster::start("migrate") else {
        return;
    };
    assert_eq!(ohl_store_pg::schema::SCHEMA, 8);
    // No backup: refused (the server and every program connecting), the
    // database unchanged.
    let url = schema_6(&cluster, "old");
    let refused = |url: &str| match Network::connect(url) {
        Err(NetError::Refused(m)) => m,
        other => panic!("{other:?}"),
    };
    let e = refused(&url);
    assert!(
        e.contains("no backup is recorded") && e.contains("--i-have-a-backup"),
        "{e}"
    );
    assert!(Network::connect_writer(&url).is_err());
    assert_eq!(
        Network::connect_as_is(&url)
            .unwrap()
            .schema_as_is()
            .unwrap(),
        6
    );
    let e = Network::connect_as_is(&url)
        .unwrap()
        .migrate_now("ana", false)
        .unwrap_err()
        .to_string();
    assert!(e.contains("no backup is recorded"), "{e}");
    // A backup older than the setting (24 h by default): refused.
    let mut c = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    c.batch_execute(
        "INSERT INTO net_meta (key, value) VALUES ('last_backup',
             json_build_object('epoch', extract(epoch FROM now()) - 30 * 3600,
                 'time', 'yesterday')::text)",
    )
    .unwrap();
    let e = refused(&url);
    assert!(e.contains("30.0 h old") && e.contains("(24)"), "{e}");
    // The network's setting allows older ones.
    c.batch_execute("INSERT INTO net_meta (key, value) VALUES ('backup_max_age_hours', '48')")
        .unwrap();
    let net = Network::connect(&url).unwrap();
    assert_eq!(net.schema().unwrap(), 8);
    let migrated = net
        .audit_rows()
        .unwrap()
        .into_iter()
        .find(|a| a[2] == "migrate")
        .unwrap();
    assert!(migrated[4].contains("6 brought to 8") && migrated[4].contains("yesterday"));

    // A real backup made by this program on the old schema lets it go.
    let url = schema_6(&cluster, "old2");
    let dir = cluster.scratch().join("old2-backup");
    let r = backup::backup(&url, "ana", &dir).unwrap();
    assert_eq!(r.manifest.schema, 6);
    let net = Network::connect_writer(&url).unwrap();
    assert_eq!(net.schema().unwrap(), 8);
    drop(net);
    // A backup of schema 6 is not restored by this program (schema 8).
    cluster.create_db("restore6");
    let e = backup::restore(&cluster.url("restore6"), "ana", &dir)
        .unwrap_err()
        .to_string();
    assert!(e.contains("schema 6") && e.contains("then migrate"), "{e}");

    // --i-have-a-backup: no backup checked, in the audit with its user.
    let url = schema_6(&cluster, "old3");
    let (from, to) = Network::connect_as_is(&url)
        .unwrap()
        .migrate_now("ana", true)
        .unwrap();
    assert_eq!((from, to), (6, 8));
    let net = Network::connect(&url).unwrap();
    let a = net
        .audit_rows()
        .unwrap()
        .into_iter()
        .find(|a| a[2] == "migrate")
        .unwrap();
    assert_eq!(a[1], "ana");
    assert!(a[4].contains("--i-have-a-backup"), "{a:?}");
    // An empty database is made without any backup.
    cluster.create_db("fresh");
    assert_eq!(
        Network::connect(&cluster.url("fresh"))
            .unwrap()
            .schema()
            .unwrap(),
        8
    );

    // The setting: network admins only, checked, in the audit.
    assert_eq!(
        net.setting("backup_max_age_hours").unwrap().as_deref(),
        Some("24")
    );
    use ohl_store_pg::rights::RightsCommand as R;
    net.rights_command(
        "ana",
        &R::AddUser {
            name: "ben".into(),
            display: None,
        },
    )
    .unwrap();
    assert!(
        net.set_setting("ben", "backup_max_age_hours", "48")
            .is_err()
    );
    assert!(
        net.set_setting("ana", "backup_max_age_hours", "-1")
            .is_err()
    );
    assert!(
        net.set_setting("ana", "backup_max_age_hours", "NaN")
            .is_err()
    );
    assert!(net.set_setting("ana", "tile_level", "3").is_err());
    net.set_setting("ana", "backup_max_age_hours", "72")
        .unwrap();
    assert_eq!(
        net.setting("backup_max_age_hours").unwrap().as_deref(),
        Some("72")
    );
    assert!(
        net.audit_rows()
            .unwrap()
            .iter()
            .any(|a| a[2] == "setting" && a[4] == "24 → 72")
    );
}

/// Phase 13: invitations reissued and identities mapped, by network admins
/// only, in the audit.
#[test]
fn invitations_and_identities() {
    let Some(cluster) = Cluster::start("identities") else {
        return;
    };
    cluster.create_db("net");
    let net = Network::connect(&cluster.url("net")).unwrap();
    use ohl_store_pg::rights::RightsCommand as R;
    net.rights_command("ana", &R::Bootstrap { user: "ana".into() })
        .unwrap();
    for name in ["ben", "cy"] {
        net.rights_command(
            "ana",
            &R::AddUser {
                name: name.into(),
                display: None,
            },
        )
        .unwrap();
    }
    assert_eq!(net.invitation_generations().unwrap()["ben"], 0);
    let rev = net.rights_rev().unwrap();
    net.rights_command("ana", &R::ReissueInvitation { name: "ben".into() })
        .unwrap();
    assert_eq!(net.invitation_generations().unwrap()["ben"], 1);
    assert!(net.rights_rev().unwrap() > rev);
    assert!(
        net.rights_command("ben", &R::ReissueInvitation { name: "cy".into() })
            .is_err()
    );
    assert!(
        net.rights_command("ana", &R::ReissueInvitation { name: "zed".into() })
            .is_err()
    );
    let map = R::MapIdentity {
        issuer: "https://id.example.org".into(),
        subject: "00u1".into(),
        user: "ben".into(),
    };
    assert!(net.rights_command("ben", &map).is_err());
    net.rights_command("ana", &map).unwrap();
    assert_eq!(
        net.identity_user("https://id.example.org", "00u1")
            .unwrap()
            .as_deref(),
        Some("ben")
    );
    assert_eq!(
        net.identity_user("https://other.example.org", "00u1")
            .unwrap(),
        None
    );
    assert_eq!(net.identities().unwrap().len(), 1);
    net.rights_command(
        "ana",
        &R::UnmapIdentity {
            issuer: "https://id.example.org".into(),
            subject: "00u1".into(),
        },
    )
    .unwrap();
    assert_eq!(
        net.identity_user("https://id.example.org", "00u1").unwrap(),
        None
    );
    let audit = net.audit_rows().unwrap();
    for action in [
        "rights.reissue_invitation",
        "rights.map_identity",
        "rights.unmap_identity",
    ] {
        assert!(audit.iter().any(|a| a[2] == action), "{action}");
    }
}

/// TLS to the database: the cluster given a certificate of the owner's CA
/// (for 127.0.0.1 and localhost); `verify-full` and `verify-ca` with that
/// CA, `require`; another CA and a name the certificate is not for
/// refused; the session's connection encrypted.
#[test]
fn tls_to_the_database() {
    let Some(cluster) = Cluster::start("tls") else {
        return;
    };
    use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};
    let dir = cluster.scratch();
    let make_ca = || {
        let mut p = CertificateParams::new(Vec::<String>::new()).unwrap();
        p.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        rcgen::CertifiedIssuer::self_signed(p, KeyPair::generate().unwrap()).unwrap()
    };
    let ca = make_ca();
    let other_ca = make_ca();
    let key = KeyPair::generate().unwrap();
    let cert = CertificateParams::new(vec!["localhost".to_owned(), "127.0.0.1".to_owned()])
        .unwrap()
        .signed_by(&key, &*ca)
        .unwrap();
    std::fs::write(dir.join("ca.pem"), ca.pem()).unwrap();
    std::fs::write(dir.join("other-ca.pem"), other_ca.pem()).unwrap();
    std::fs::write(dir.join("server.crt"), cert.pem()).unwrap();
    let key_file = dir.join("server.key");
    {
        use std::io::Write;
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        o.open(&key_file)
            .unwrap()
            .write_all(key.serialize_pem().as_bytes())
            .unwrap();
    }
    let mut admin = cluster.admin();
    // ALTER SYSTEM one statement at a time (never in a transaction).
    let set = |admin: &mut postgres::Client, on: bool| {
        let statements = if on {
            vec![
                format!(
                    "ALTER SYSTEM SET ssl_cert_file = '{}'",
                    dir.join("server.crt").display()
                ),
                format!("ALTER SYSTEM SET ssl_key_file = '{}'", key_file.display()),
                "ALTER SYSTEM SET ssl = on".to_owned(),
            ]
        } else {
            vec![
                "ALTER SYSTEM RESET ssl".to_owned(),
                "ALTER SYSTEM RESET ssl_cert_file".to_owned(),
                "ALTER SYSTEM RESET ssl_key_file".to_owned(),
            ]
        };
        for s in statements {
            admin.simple_query(&s).unwrap();
        }
        admin.simple_query("SELECT pg_reload_conf()").unwrap();
    };
    set(&mut admin, true);
    cluster.create_db("net");
    let base = cluster.url("net");
    let with = |extra: &str| format!("{base} {extra}");
    let ca_file = dir.join("ca.pem").display().to_string();
    // The reload is not immediate: wait for TLS to be offered.
    let started = std::time::Instant::now();
    loop {
        let t = ohl_store_pg::connect::Target::parse(&with(&format!(
            "sslmode=verify-full sslrootcert='{ca_file}'"
        )))
        .unwrap();
        if t.connect().is_ok() {
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "TLS never offered");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let encrypted = |url: &str| -> Result<bool, String> {
        let mut c = ohl_store_pg::connect::Target::parse(url)
            .map_err(|e| e.to_string())?
            .connect()
            .map_err(|e| e.to_string())?;
        Ok(c.query_one(
            "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()",
            &[],
        )
        .unwrap()
        .get(0))
    };
    let full = with(&format!("sslmode=verify-full sslrootcert='{ca_file}'"));
    assert_eq!(encrypted(&full), Ok(true));
    assert_eq!(
        encrypted(&with(&format!("sslmode=verify-ca sslrootcert='{ca_file}'"))),
        Ok(true)
    );
    assert_eq!(encrypted(&with("sslmode=require")), Ok(true));
    // Without TLS on this computer: allowed, not encrypted.
    assert_eq!(encrypted(&base), Ok(false));
    // Another CA: refused.
    let other = dir.join("other-ca.pem").display().to_string();
    let e = encrypted(&with(&format!("sslmode=verify-full sslrootcert='{other}'"))).unwrap_err();
    assert!(e.to_lowercase().contains("certificate"), "{e}");
    // The network over TLS: made, and its writer.
    let net = Network::connect_writer(&full).unwrap();
    net.create_line("A", "a", "").unwrap();
    drop(net);
    // A name the certificate is not for (only 127.0.0.1 and localhost):
    // verify-full refuses, verify-ca takes it.
    let named = |mode: &str| {
        base.replacen("host=127.0.0.1", "host=db.invalid hostaddr=127.0.0.1", 1)
            + &format!(" sslmode={mode} sslrootcert='{ca_file}'")
    };
    assert!(encrypted(&named("verify-full")).is_err());
    assert_eq!(encrypted(&named("verify-ca")), Ok(true));
    set(&mut admin, false);
}
