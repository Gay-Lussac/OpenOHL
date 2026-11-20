// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Compose files of a country server (ADR-0013, phase 13;
//! `deploy/compose`), read as YAML (no Docker here): the database with
//! PostGIS, the server and the backup job, each with a healthcheck; the
//! volumes and secrets they name declared; the passwords only from secret
//! files, the example's clearly fake; the server's commands ones `ohl`
//! knows.

use std::path::Path;

use yaml_rust2::{Yaml, YamlLoader};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn compose() -> Yaml {
    let text = std::fs::read_to_string(root().join("deploy/compose/compose.yaml")).unwrap();
    YamlLoader::load_from_str(&text).unwrap().remove(0)
}

fn keys(y: &Yaml) -> Vec<String> {
    y.as_hash()
        .map(|h| {
            h.keys()
                .filter_map(|k| k.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn compose_files_hold_together() {
    let c = compose();
    let services = &c["services"];
    assert_eq!(keys(services), ["db", "server", "backup"]);
    let volumes = keys(&c["volumes"]);
    let secrets = keys(&c["secrets"]);
    for name in ["db", "server", "backup"] {
        let s = &services[name];
        assert!(
            !s["healthcheck"]["test"].is_badvalue(),
            "{name}: a healthcheck"
        );
        assert_eq!(s["restart"].as_str(), Some("unless-stopped"), "{name}");
        // Every named volume and secret declared.
        for v in s["volumes"].as_vec().unwrap() {
            let v = v.as_str().unwrap();
            let source = v.split(':').next().unwrap();
            if source.starts_with('.') {
                assert!(
                    root().join("deploy/compose").join(source).exists(),
                    "{name}: {source}"
                );
            } else {
                assert!(
                    volumes.contains(&source.to_owned()),
                    "{name}: volume {source}"
                );
            }
        }
        for sec in s["secrets"].as_vec().unwrap() {
            assert!(secrets.contains(&sec.as_str().unwrap().to_owned()));
        }
        // No password in the file itself: only *_FILE settings.
        for (k, v) in s["environment"].as_hash().unwrap() {
            let (k, v) = (k.as_str().unwrap(), v.as_str().unwrap_or_default());
            if k.contains("PASSWORD") {
                assert!(
                    k.ends_with("_FILE") && v.starts_with("/run/secrets/"),
                    "{k}"
                );
            }
            assert!(!v.contains("password="), "{k}");
        }
    }
    // The database not published; the server's QUIC port published.
    assert!(services["db"]["ports"].is_badvalue());
    assert_eq!(
        services["server"]["ports"][0].as_str(),
        Some("4433:4433/udp")
    );
    // The server and the job depend on a healthy database.
    for name in ["server", "backup"] {
        assert_eq!(
            services[name]["depends_on"]["db"]["condition"].as_str(),
            Some("service_healthy")
        );
    }
    // The secrets' example files: present, clearly fake.
    for (name, s) in c["secrets"].as_hash().unwrap() {
        let file = s["file"].as_str().unwrap();
        let example = file.replace("./secrets/", "secrets.example/");
        let text = std::fs::read_to_string(root().join("deploy/compose").join(&example)).unwrap();
        assert!(text.starts_with("EXAMPLE-ONLY"), "{name:?}: {example}");
    }
    // The server's command is one `ohl` takes.
    let cmd: Vec<&str> = services["server"]["command"]
        .as_vec()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(&cmd[..2], ["network", "serve"]);
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["network", "serve", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    for flag in cmd.iter().filter(|a| a.starts_with("--")) {
        assert!(help.contains(flag), "ohl network serve has no {flag}");
    }
    for flag in ["--oidc-issuer", "--oidc-audience", "--invite-days"] {
        assert!(help.contains(flag), "{flag}");
    }
    // The backup job's script uses a command `ohl` has.
    let script = std::fs::read_to_string(root().join("deploy/compose/backup-loop.sh")).unwrap();
    assert!(script.contains("ohl network backup --out"));
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["network", "backup", "--help"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&help.stdout).contains("--out"));
}
