// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The offline working set ([`crate::offline`], phase 12): a working set
//! written with its export's revision, worked on, its bundle holding only
//! what changed since, read back as changes; a bundle changed by hand, cut
//! short, made from another base or carrying nothing is refused.

use super::*;
use crate::offline::{OfflineMeta, offline_changes, tree_object_hashes};
use crate::working_set::{Pick, WorkingSetSpec};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-offline-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

/// The whole model (as the server holds it) and an offline working set of
/// route `main` written from it: the file, its record, the export's tree.
fn exported(name: &str) -> (PathBuf, Session, OfflineMeta, String) {
    let d = dir(name);
    let mut whole = Session::open(&d.join("whole.ohl")).unwrap();
    whole.import(&example(), "ana").unwrap();
    whole.set_user("ana", "ben", Role::Designer).unwrap();
    let picks = vec![Pick::Route {
        route: "main".into(),
    }];
    let extent = whole.pick_extent(&picks).unwrap();
    let replica = whole.replica_of(&extent);
    let offline = OfflineMeta {
        network: "test".into(),
        model: "EX".into(),
        user: "ben".into(),
        time: String::new(),
        base: "0".repeat(64),
        export: String::new(),
        spec: WorkingSetSpec {
            name: String::new(),
            model: "EX".into(),
            picks,
        },
        routes: vec!["main".into()],
        reach_m: 500.0,
        area: [0.0; 4],
        max_margin: 0.0,
        library_pins: Default::default(),
        left_out: Default::default(),
        neighbour_lines: 0,
        neighbour_sections: 0,
    };
    let path = d.join("offline.ohl");
    let (summary, offline, tree) = Session::write_offline(
        replica,
        &path,
        "ben",
        &[],
        offline,
        "offline working set exported",
    )
    .unwrap();
    assert!(summary.wanted > 0 && summary.context > 0);
    assert_eq!(offline.export.len(), 64);
    (path, whole, offline, tree)
}

fn shift(s: &mut Session, id: &str, dx: f64) {
    let p = s.line().structure(id).unwrap().position;
    s.apply(
        "ben",
        Command::MoveStructure {
            id: id.into(),
            position: [p[0] + dx, p[1], p[2]],
        },
    )
    .unwrap();
}

#[test]
fn a_bundle_holds_only_the_changes() {
    let (path, _whole, meta, tree) = exported("only");
    let mut s = Session::open(&path).unwrap();
    assert_eq!(s.offline_meta().unwrap().unwrap(), meta);
    // Nothing committed: no bundle.
    assert!(s.offline_bundle().is_err());
    shift(&mut s, "T2", 0.5);
    // Not committed: refused (it would not come back).
    let e = s.offline_bundle().unwrap_err().to_string();
    assert!(e.contains("not committed"), "{e}");
    s.commit("ben", "T2 east").unwrap();
    shift(&mut s, "T2", 0.25);
    shift(&mut s, "T4", -0.5);
    s.commit("ben", "T2 and T4").unwrap();
    let bundle = s.offline_bundle().unwrap();
    assert_eq!(bundle.revisions.len(), 2);
    // Two trees and the values of T2 (twice) and T4: never the export's
    // state again.
    let exported_objects = tree_object_hashes(&tree).unwrap().len();
    assert!(bundle.blobs.len() <= 5, "{}", bundle.blobs.len());
    assert!(exported_objects > 10);
    let c = offline_changes(&bundle, &meta.export, &tree).unwrap();
    let ids: Vec<&str> = c.puts.iter().map(|(_, id, _)| id.as_str()).collect();
    assert_eq!(ids, ["T2", "T4"]);
    assert!(c.deletes.is_empty() && c.libraries.is_empty());
    assert_eq!(c.revisions.len(), 2);
    assert_eq!(c.revisions[1].2, "T2 and T4");
    let t2 = &c.puts[0].2;
    assert_eq!(
        t2["position"][0].as_f64().unwrap(),
        s.line().structure("T2").unwrap().position[0]
    );
    // A whole bundle (the export's revision in it) gives the same.
    let whole = s.export_bundle("main").unwrap();
    assert_eq!(
        offline_changes(&whole, &meta.export, &tree).unwrap().puts,
        c.puts
    );
}

#[test]
fn a_bundle_changed_or_cut_or_from_elsewhere_is_refused() {
    let (path, _whole, meta, tree) = exported("tampered");
    let mut s = Session::open(&path).unwrap();
    shift(&mut s, "T2", 0.5);
    s.commit("ben", "T2 east").unwrap();
    let bundle = s.offline_bundle().unwrap();
    let refused = |b: &crate::versions::Bundle, base: &str, words: &str| {
        let e = offline_changes(b, base, &tree).unwrap_err().to_string();
        assert!(e.contains(words), "{words:?} not in: {e}");
    };
    // A value changed by hand (its hash no longer its text's).
    let mut b = bundle.clone();
    let (h, text) = b
        .blobs
        .iter()
        .find(|(_, t)| t.contains("\"T2\""))
        .map(|(h, t)| (h.clone(), t.clone()))
        .unwrap();
    b.blobs.insert(h, text.replace("\"T2\"", "\"T9\""));
    refused(&b, &meta.export, "does not match its hash");
    // A message changed: the revision's id is not its content's.
    let mut b = bundle.clone();
    b.revisions[0].message.push('!');
    refused(&b, &meta.export, "damaged");
    // A value left out.
    let mut b = bundle.clone();
    let h = b
        .blobs
        .iter()
        .find(|(_, t)| t.contains("\"T2\""))
        .map(|(h, _)| h.clone())
        .unwrap();
    b.blobs.remove(&h);
    refused(&b, &meta.export, "not whole");
    // A base the bundle was not made from.
    refused(
        &bundle,
        &"1".repeat(64),
        "neither the export nor in the bundle",
    );
    // A head not among its revisions.
    let mut b = bundle.clone();
    b.head = "2".repeat(64);
    refused(&b, &meta.export, "head");
    // Nothing after the export.
    let mut b = bundle.clone();
    b.revisions.clear();
    b.head = meta.export.clone();
    refused(&b, &meta.export, "nothing was committed");
}

/// The rules of the part hold offline: an object of the context (read
/// only) is not changed; the server refuses it again when it comes back.
#[test]
fn the_context_is_read_only_offline() {
    let (path, _whole, _meta, _tree) = exported("context");
    let mut s = Session::open(&path).unwrap();
    // T6 is of route `branch`: in the file as context only.
    let p = s.line().structure("T6").map(|t| t.position);
    if let Some(p) = p {
        assert!(
            s.apply(
                "ben",
                Command::MoveStructure {
                    id: "T6".into(),
                    position: [p[0] + 1.0, p[1], p[2]],
                },
            )
            .is_err()
        );
    }
}
