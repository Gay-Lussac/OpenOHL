// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use super::*;

fn small(seed: u64) -> Country {
    country(&Settings {
        pylons: 600,
        seed,
        ..Settings::default()
    })
    .unwrap()
}

/// The line's SHA-256 per seed (600 pylons): a change of the generator
/// that changes its output must change these on purpose (and the
/// baselines measured on it are then measured again).
#[test]
fn hash_per_seed() {
    for (seed, hash) in [
        (
            1,
            "1fe927632b9e382bc1f1daa9b5d9a3cb964184c96773a0e33ab0dd7bc2df9999",
        ),
        (
            2,
            "92b87ae8e443001daa5e9b94fa60f400c6b170562a2b195caea661e41a2c6cae",
        ),
    ] {
        let c = small(seed);
        assert_eq!(c.summary.hash, hash, "seed {seed}: {:#?}", c.summary);
        assert_eq!(c.hash(), hash);
    }
}

#[test]
fn same_seed_same_country_other_seed_another() {
    let a = small(7);
    let b = small(7);
    assert_eq!(a.line, b.line);
    assert_eq!(a.summary, b.summary);
    assert_ne!(small(8).summary.hash, a.summary.hash);
}

#[test]
fn counts_follow_the_settings() {
    let c = country(&Settings {
        pylons: 3000,
        ..Settings::default()
    })
    .unwrap();
    let s = &c.summary;
    assert_eq!(s.pylons, 3000);
    assert_eq!(c.line.structures.len(), 3000);
    assert_eq!(c.line.sections.len(), s.sections);
    assert_eq!(c.line.jumpers.len(), s.jumpers);
    assert!((2.1..2.9).contains(&s.systems_per_pylon), "{s:#?}");
    assert!((0.04..0.2).contains(&s.shared_share), "{s:#?}");
    // Sections of about two spans (the plan's proportions).
    let spans = s.wire_spans as f64 / s.sections as f64;
    assert!((1.5..2.6).contains(&spans), "{spans}");
    // Every pylon within the plane's extent, grown by the longest route.
    assert!(c.line.structures.iter().all(|t| t.position[0] > 0.0));
}

#[test]
fn lidar_tiles_are_deterministic_and_dense_as_asked() {
    let c = small(1);
    let corner = c.sample_tiles[0];
    let a = lidar_tile(1, corner, 0.5);
    let b = lidar_tile(1, corner, 0.5);
    assert_eq!(a.len(), (0.5 * TILE * TILE) as usize);
    assert!(
        a.iter()
            .zip(&b)
            .all(|(p, q)| p.x == q.x && p.z == q.z && p.class == q.class)
    );
    assert!(a.iter().any(|p| p.class == 5) && a.iter().any(|p| p.class == 2));
    assert!(a.iter().all(|p| p.x >= corner[0] && p.x < corner[0] + TILE));
}

#[test]
fn written_project_opens_and_takes_an_edit() {
    let dir = std::env::temp_dir().join(format!("ohl-synth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let c = country(&Settings {
        pylons: 120,
        ..Settings::default()
    })
    .unwrap();
    let path = dir.join("c.ohl");
    project::write_project(&c, &path, "synth").unwrap();
    let tiles = project::add_lidar(&c, &path, 0.2, std::time::Duration::from_secs(60)).unwrap();
    assert_eq!(tiles.len(), c.sample_tiles.len());
    assert!(tiles.iter().all(|t| t.bytes > 0));
    let b = project::measure(&path, "synth", 2, std::time::Duration::from_secs(60)).unwrap();
    assert_eq!(b.move_s.len(), 2);
    assert_eq!(b.undo_s.len(), 2);
    assert!(b.stopped.is_none());
    let session = ohl_session::Session::open(&path).unwrap();
    assert_eq!(session.line().structures.len(), 120);
    // Undone: as generated.
    assert_eq!(
        session.line().structures[1].position,
        c.line.structures[1].position
    );
    drop(session);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The owner's size; about 20 s (most of it the line's validation), run
/// by hand: `cargo test -p ohl-synth --release -- --ignored`.
#[test]
#[ignore]
fn owner_size() {
    let t = std::time::Instant::now();
    let c = country(&Settings::default()).unwrap();
    println!("generated in {:?}: {:#?}", t.elapsed(), c.summary);
    assert_eq!(
        c.summary.hash,
        "1c14a252dda1b8d1e397fb5675a3600ff7a792d7d4af4a128b1de3a02b053d7a"
    );
}
