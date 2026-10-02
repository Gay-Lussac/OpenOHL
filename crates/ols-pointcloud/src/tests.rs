// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use las::point::Classification;
use las::{Builder, Point, Writer};

/// A LAZ tile: 0.25 m grid points (1 mm coordinates) over [x0, x0 + w) ×
/// [0, 40), ground below a plane, every 7th point vegetation 8 m above.
fn write_tile(path: &Path, x0: f64, w: f64) {
    let mut builder = Builder::from((1, 4));
    builder.point_format.is_compressed = true;
    builder.transforms.x.scale = 0.001;
    builder.transforms.y.scale = 0.001;
    builder.transforms.z.scale = 0.001;
    let mut writer = Writer::from_path(path, builder.into_header().unwrap()).unwrap();
    let n = (w / 0.25) as usize;
    for i in 0..n {
        for j in 0..160 {
            let (x, y) = (x0 + i as f64 * 0.25, j as f64 * 0.25);
            let z = ((500.0 + 0.03 * x - 0.01 * y) * 1000.0).round() / 1000.0;
            let veg = (i + j) % 7 == 0;
            writer
                .write_point(Point {
                    x,
                    y,
                    z: if veg { z + 8.0 } else { z },
                    intensity: (i * 7 + j) as u16,
                    return_number: 1,
                    number_of_returns: 1,
                    classification: if veg {
                        Classification::HighVegetation
                    } else {
                        Classification::Ground
                    },
                    ..Default::default()
                })
                .unwrap();
        }
    }
    writer.close().unwrap();
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ols-pc-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sorted(mut v: Vec<[i64; 4]>) -> Vec<[i64; 4]> {
    v.sort_unstable();
    v
}

fn collect(
    source: &dyn PointSource,
    area: Option<&Corridor>,
    classes: Option<&[u8]>,
) -> Vec<[i64; 4]> {
    let mut out = Vec::new();
    source
        .visit(area, classes, &Budget::new(), &mut |x, y, z, c| {
            let mm = |v: f64| (v * 1000.0).round() as i64;
            out.push([mm(x), mm(y), mm(z), c as i64]);
        })
        .unwrap();
    sorted(out)
}

#[test]
fn store_returns_exactly_the_tiles_points() {
    let dir = scratch("same");
    // Two tiles splitting the chunk [50, 100) at x = 70.
    write_tile(&dir.join("a.laz"), 0.0, 70.0);
    write_tile(&dir.join("b.laz"), 70.0, 50.0);
    let tiles = ols_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("project.ols");
    let mut store = Store::open(&db).unwrap();
    let (cloud, stats) = store
        .import_tiles(
            "survey",
            &tiles,
            "EPSG:2056",
            Some("id = \"x\""),
            50.0,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    assert_eq!(stats.read, 2);
    assert_eq!(cloud.points, (280 + 200) * 160);
    drop(store);

    // Reopened: same points as the tiles, with and without filters.
    let from_store = CloudSource::open(&db, None).unwrap();
    assert_eq!(from_store.class_scheme().as_deref(), Some("id = \"x\""));
    let from_tiles = TileSource {
        tiles: tiles.clone(),
        scheme: None,
    };
    assert_eq!(
        collect(&from_store, None, None),
        collect(&from_tiles, None, None)
    );
    let ground = [2u8];
    let corridor = Corridor::new(&[vec![[-5.0, 10.0], [130.0, 10.0]]], 3.0);
    let a = collect(&from_store, Some(&corridor), Some(&ground));
    assert_eq!(a, collect(&from_tiles, Some(&corridor), Some(&ground)));
    assert!(
        !a.is_empty()
            && a.iter()
                .all(|p| p[3] == 2 && (p[1] - 10_000).abs() <= 3_000)
    );
    // Only the ground blocks of the touched chunks were read.
    let mut n = 0;
    let stats = from_store
        .visit(
            Some(&corridor),
            Some(&ground),
            &Budget::new(),
            &mut |_, _, _, _| n += 1,
        )
        .unwrap();
    assert_eq!((stats.read, stats.skipped), (3, 3), "{stats:?}");
    assert_eq!(stats.points_visited, n);
    let counts = Store::open(&db)
        .unwrap()
        .class_counts(from_store.cloud())
        .unwrap();
    assert_eq!(counts.iter().map(|c| c.1).sum::<u64>(), cloud.points);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn several_clouds_and_newer_schema() {
    let dir = scratch("clouds");
    write_tile(&dir.join("a.laz"), 0.0, 10.0);
    let tiles = ols_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("p.ols");
    let mut store = Store::open(&db).unwrap();
    for name in ["2023", "2025"] {
        store
            .import_tiles(
                name,
                &tiles,
                "",
                None,
                50.0,
                0.001,
                &Budget::new(),
                |_, _, _| {},
            )
            .unwrap();
    }
    assert!(store.cloud(None).unwrap_err().0.contains("several"));
    assert_eq!(store.cloud(Some("2025")).unwrap().points, 40 * 160);
    store.delete_cloud("2023").unwrap();
    assert_eq!(store.cloud(None).unwrap().name, "2025");
    store
        .conn
        .execute(
            "UPDATE ols_meta SET value = '99' WHERE key = 'pointcloud_schema'",
            [],
        )
        .unwrap();
    drop(store);
    assert!(Store::open(&db).unwrap_err().0.contains("newer"));
    std::fs::remove_dir_all(dir).ok();
}
