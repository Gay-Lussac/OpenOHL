// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
    // Projected coordinates need an offset to fit the 32-bit integers.
    builder.transforms.x.offset = x0.floor();
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
    let dir = std::env::temp_dir().join(format!("ohl-pc-{}-{name}", std::process::id()));
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
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("project.ohl");
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
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("p.ohl");
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

#[test]
fn points_per_grid_tile() {
    let dir = scratch("gridtiles");
    // Points over x 0–120 m, y 0–40 m (a tile of 120 × 40 m).
    write_tile(&dir.join("a.laz"), 0.0, 120.0);
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("p.ohl");
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_tiles(
            "survey",
            &tiles,
            "",
            None,
            DEFAULT_CHUNK,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    let grid = ohl_core::grid::Grid::new(128.0).unwrap();
    // Level 0 (128 m): one tile with every point, chunks nested.
    let t = store.tiles(&cloud, &grid, 0).unwrap();
    assert!(t.exact);
    assert_eq!(t.tiles.len(), 1);
    assert_eq!(t.tiles[0].1, cloud.points);
    // Level −1 (64 m, the chunk size): two tiles along x, still exact.
    let t = store.tiles(&cloud, &grid, -1).unwrap();
    assert!(t.exact);
    assert_eq!(t.tiles.iter().map(|x| x.0.x).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(t.tiles.iter().map(|x| x.1).sum::<u64>(), cloud.points);
    // Finer than the chunks: counted by chunk centre, not exact.
    assert!(!store.tiles(&cloud, &grid, -2).unwrap().exact);
    // A corridor reads only its chunks' blocks.
    let source = CloudSource::open(&db, None).unwrap();
    let corridor = Corridor::new(&[vec![[100.0, 5.0], [110.0, 5.0]]], 2.0);
    let stats = source
        .visit(Some(&corridor), None, &Budget::new(), &mut |x, _, _, _| {
            assert!((98.0..=112.0).contains(&x))
        })
        .unwrap();
    let blocks: usize = store
        .conn
        .query_row("SELECT COUNT(*) FROM pointcloud_block", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap() as usize;
    assert_eq!(stats.read + stats.skipped, blocks);
    assert!(stats.read < blocks, "{stats:?}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_rectangle_gives_exactly_its_points() {
    let dir = scratch("rect");
    write_tile(&dir.join("a.laz"), 0.0, 120.0);
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("project.ohl");
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_tiles(
            "survey",
            &tiles,
            "EPSG:2056",
            None,
            50.0,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    let all = collect(&CloudSource::open(&db, None).unwrap(), None, None);
    // Two rectangles side by side at x = 64 (not a chunk border): every
    // point in one of them, none twice.
    let mut parts = Vec::new();
    for (min, max) in [([0.0, 0.0], [64.0, 40.0]), ([64.0, 0.0], [130.0, 40.0])] {
        let mut out = Vec::new();
        let stats = store
            .visit_rect(&cloud, min, max, None, &Budget::new(), &mut |x, y, z, c| {
                let mm = |v: f64| (v * 1000.0).round() as i64;
                assert!(x >= min[0] && x < max[0]);
                out.push([mm(x), mm(y), mm(z), c as i64]);
            })
            .unwrap();
        assert_eq!(stats.points_visited as usize, out.len());
        parts.extend(out);
    }
    assert_eq!(sorted(parts), all);
    // One class only.
    let mut n = 0;
    store
        .visit_rect(
            &cloud,
            [0.0, 0.0],
            [64.0, 40.0],
            Some(&[5]),
            &Budget::new(),
            &mut |_, _, _, c| {
                assert_eq!(c, 5);
                n += 1;
            },
        )
        .unwrap();
    assert!(n > 0);
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn damaged_block_counts_error_cleanly() {
    let dir = scratch("damaged");
    write_tile(&dir.join("a.laz"), 0.0, 10.0);
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("project.ohl");
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_tiles(
            "survey",
            &tiles,
            "",
            None,
            50.0,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    // Negative, beyond the cap, and within it but not what the blobs hold
    // (no intensities nor returns stored).
    for count in [-1i64, i64::MAX, 4_000_000_000, 90_000_000] {
        store
            .conn
            .execute(
                "UPDATE pointcloud_block SET count = ?1, intensity = NULL, returns = NULL",
                [count],
            )
            .unwrap();
        let read = store.visit_rect(
            &cloud,
            [0.0, 0.0],
            [10.0, 40.0],
            None,
            &Budget::new(),
            &mut |_, _, _, _| {},
        );
        assert!(read.is_err(), "count {count}");
    }
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

/// Chunks that do not nest in the grid's tiles (50 m chunks, 64 m tiles):
/// every point's tile is listed, its points at or above the tile's lowest
/// block base and the highest base below the top point.
#[test]
fn tile_extents_cover_chunks_across_tile_borders() {
    let dir = scratch("extents");
    write_tile(&dir.join("a.laz"), 0.0, 120.0);
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("project.ohl");
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_tiles(
            "survey",
            &tiles,
            "EPSG:2056",
            None,
            50.0,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    let grid = ohl_core::grid::Grid::new(64.0).unwrap();
    let extents: std::collections::BTreeMap<_, _> = store
        .tile_extents(&cloud, &grid, 0)
        .unwrap()
        .into_iter()
        .collect();
    let mut low: std::collections::BTreeMap<ohl_core::grid::Tile, f64> = Default::default();
    store
        .visit(&cloud, None, None, &Budget::new(), &mut |x, y, z, _| {
            let t = grid.tile_at(0, [x, y]);
            let e = extents.get(&t).expect("every point's tile listed");
            assert!(z >= e[0] - 1e-6, "{z} below the lowest base {}", e[0]);
            let l = low.entry(t).or_insert(z);
            *l = l.min(z);
        })
        .unwrap();
    // Tiles x 0..1 (0-128 m wide survey of 120 m): both listed.
    assert!(extents.keys().any(|t| t.x == 1));
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

/// A small LAS 1.4 tile (point format 6) with classes above 31 and 12.
fn write_high_classes(path: &Path) {
    let mut b = Builder::from((1, 4));
    b.point_format = las::point::Format::new(6).unwrap();
    for t in [
        &mut b.transforms.x,
        &mut b.transforms.y,
        &mut b.transforms.z,
    ] {
        t.scale = 0.001;
    }
    let mut w = Writer::from_path(path, b.into_header().unwrap()).unwrap();
    for (i, class) in [12u8, 31, 72, 98].into_iter().enumerate() {
        for k in 0..50 {
            w.write_point(Point {
                x: 10.0 + f64::from(k) * 0.5,
                y: 5.0 + i as f64,
                z: 400.0 + i as f64,
                intensity: 100 + k as u16,
                return_number: 1,
                number_of_returns: 2,
                classification: Classification::new(class)
                    .unwrap_or(Classification::Reserved(class)),
                gps_time: Some(0.0),
                ..Default::default()
            })
            .unwrap();
        }
    }
    w.close().unwrap();
}

/// Exported to LAZ and imported again: the same points, classes (above
/// 31 too) and intensities; classes and an area can be chosen.
#[test]
fn export_and_import_again_give_the_same_points() {
    let dir = scratch("export");
    write_tile(&dir.join("a.laz"), 0.0, 60.0);
    write_high_classes(&dir.join("b.las"));
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("project.ohl");
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_tiles(
            "survey",
            &tiles,
            "EPSG:2056",
            None,
            50.0,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    let out = dir.join("out").join("export.laz");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let mut last = (0, 0);
    let stats = store
        .export_las(&cloud, &out, None, None, &Budget::new(), |a, b| {
            last = (a, b)
        })
        .unwrap();
    assert_eq!(stats.points_visited, cloud.points);
    assert_eq!(last, (cloud.points, cloud.points));
    // Imported again into another project: the same points.
    let db2 = dir.join("again.ohl");
    let mut again = Store::open(&db2).unwrap();
    let (cloud2, _) = again
        .import_tiles(
            "back",
            std::slice::from_ref(&out),
            "EPSG:2056",
            None,
            50.0,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    assert_eq!(cloud2.points, cloud.points);
    drop(again);
    assert_eq!(
        collect(&CloudSource::open(&db2, None).unwrap(), None, None),
        collect(&CloudSource::open(&db, None).unwrap(), None, None)
    );
    assert_eq!(
        Store::open(&db2).unwrap().class_counts(&cloud2).unwrap(),
        store.class_counts(&cloud).unwrap()
    );
    // Intensities kept.
    let mut intensities = Vec::new();
    ohl_lidar::for_each_record(std::slice::from_ref(&out), None, &Budget::new(), |p| {
        if p.class == 72 {
            intensities.push(p.intensity);
        }
    })
    .unwrap();
    assert_eq!(intensities, (100..150).collect::<Vec<u16>>());
    // Only class 98, in a rectangle.
    let part = dir.join("out").join("part.las");
    let stats = store
        .export_las(
            &cloud,
            &part,
            Some(&[98, 72]),
            Some(([10.0, 8.0], [20.0, 9.0])),
            &Budget::new(),
            |_, _| {},
        )
        .unwrap();
    assert_eq!(stats.points_visited, 20, "class 98 (y = 8), x 10-20 m");
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn any_chunk_size_far_from_the_origin() {
    // A chunk size that is not a whole number of millimetres, at
    // projected coordinates: the store still returns the tiles' points.
    let dir = scratch("odd-chunk");
    write_tile(&dir.join("a.laz"), 2_600_000.0, 40.0);
    let tiles = ohl_lidar::tiles_in(&dir).unwrap();
    let db = dir.join("project.ohl");
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_tiles(
            "survey",
            &tiles,
            "EPSG:2056",
            None,
            2.000_4,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    assert!(
        (cloud.chunk_size - 2.0).abs() < 1e-9,
        "{}",
        cloud.chunk_size
    );
    drop(store);
    let from_store = CloudSource::open(&db, None).unwrap();
    let from_tiles = TileSource {
        tiles: tiles.clone(),
        scheme: None,
    };
    assert_eq!(
        collect(&from_store, None, None),
        collect(&from_tiles, None, None)
    );
    let corridor = Corridor::new(&[vec![[2_600_005.0, 10.0], [2_600_035.0, 10.0]]], 3.0);
    let a = collect(&from_store, Some(&corridor), None);
    assert!(!a.is_empty());
    assert_eq!(a, collect(&from_tiles, Some(&corridor), None));
    std::fs::remove_dir_all(dir).ok();
}

/// Synthetic survey: a 0.5 m grid over [x0, x0 + w) × [0, h) at
/// projected coordinates, a sloping ground, every 5th point vegetation
/// 6 m above, every 997th a wire point 20 m above (class 14).
fn synthetic(x0: f64, w: usize, h: usize) -> Vec<LidarPoint> {
    let mut out = Vec::with_capacity(4 * w * h);
    for i in 0..2 * w {
        for j in 0..2 * h {
            let (x, y) = (x0 + i as f64 * 0.5, 1_200_000.0 + j as f64 * 0.5);
            let k = i * 2 * h + j;
            let ground = 400.0 + 0.02 * i as f64 + 0.01 * j as f64;
            let (z, class) = if k.is_multiple_of(997) {
                (ground + 20.0, 14)
            } else if k.is_multiple_of(5) {
                (ground + 6.0, 5)
            } else {
                (ground, 2)
            };
            out.push(LidarPoint {
                x,
                y,
                z: (z * 1000.0f64).round() / 1000.0,
                class,
                intensity: (k % 4000) as u16,
                return_number: 1,
                number_of_returns: if class == 5 { 2 } else { 1 },
            });
        }
    }
    out
}

fn mm(points: &[LidarPoint]) -> Vec<[i64; 4]> {
    let m = |v: f64| (v * 1000.0).round() as i64;
    sorted(
        points
            .iter()
            .map(|p| [m(p.x), m(p.y), m(p.z), p.class as i64])
            .collect(),
    )
}

fn point(x: f64, y: f64, z: f64, class: u8) -> LidarPoint {
    LidarPoint {
        x,
        y,
        z,
        class,
        intensity: 0,
        return_number: 1,
        number_of_returns: 1,
    }
}

#[test]
fn crafted_copc_point_counts_error_cleanly() {
    let dir = scratch("copc-crafted");
    let path = dir.join("small.copc.laz");
    copc::write_copc(
        &path,
        &synthetic(2_600_000.0, 10, 10),
        0.001,
        &Budget::new(),
    )
    .unwrap();
    let info = copc::CopcFile::open(&path).unwrap().info;
    let good = std::fs::read(&path).unwrap();
    let page = info.root_hier_offset as usize;
    // The first entry with points: its byte size at 24, its count at 28.
    let entry = (0..info.root_hier_size as usize / 32)
        .map(|k| page + 32 * k)
        .find(|&at| i32::from_le_bytes(good[at + 28..at + 32].try_into().unwrap()) > 0)
        .unwrap();
    for (byte_size, count) in [(1, i32::MAX), (-5, 10), (0, 1)] {
        let mut bad = good.clone();
        bad[entry + 24..entry + 28].copy_from_slice(&i32::to_le_bytes(byte_size));
        bad[entry + 28..entry + 32].copy_from_slice(&i32::to_le_bytes(count));
        let crafted = dir.join("crafted.copc.laz");
        std::fs::write(&crafted, &bad).unwrap();
        let err = copc::CopcFile::open(&crafted).unwrap_err();
        assert!(err.contains("points in"), "{err}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn copc_written_and_read_back() {
    let dir = scratch("copc");
    let path = dir.join("survey.copc.laz");
    // 160 m × 300 m at 0.5 m: 192 000 points, more than a node holds.
    let points = synthetic(2_600_000.0, 160, 300);
    let written = copc::write_copc(&path, &points, 0.001, &Budget::new()).unwrap();
    assert_eq!(written.points, points.len() as u64);
    assert!(written.depth >= 1 && written.nodes > 1, "{written:?}");
    assert!(copc::is_copc(&path));
    let source = copc::CopcSource::open(&path).unwrap();
    let f = &source.file;
    assert_eq!(f.points, points.len() as u64);
    assert_eq!(f.nodes.iter().map(|n| n.points).sum::<u64>(), f.points);
    assert_eq!(f.levels().iter().map(|l| l.2).sum::<u64>(), f.points);
    assert!((f.spacing(1) - f.info.spacing / 2.0).abs() < 1e-12);
    // Every node's points inside its cube.
    let mut reader = f.reader().unwrap();
    for n in &f.nodes {
        let b = f.node_bounds(&n.key);
        f.read_node(&mut reader, n, &mut |p| {
            for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                assert!(v >= b[0][k] - 1e-6 && v <= b[1][k] + 1e-6, "{:?}", n.key);
            }
        })
        .unwrap();
    }
    // Every point back, with its attributes.
    let mut all = Vec::new();
    source
        .visit_records(None, None, &Budget::new(), &mut |p| all.push(*p))
        .unwrap();
    assert_eq!(mm(&all), mm(&points));
    let p0 = all.iter().find(|p| p.class == 5).unwrap();
    assert_eq!((p0.return_number, p0.number_of_returns), (1, 2));
    // A rectangle, a corridor, a class: exactly theirs.
    let (lo, hi) = ([2_600_010.0, 1_200_020.0], [2_600_050.0, 1_200_040.0]);
    let mut rect = Vec::new();
    let stats = source
        .visit_rect(lo, hi, None, None, &Budget::new(), &mut |x, y, z, c| {
            rect.push(point(x, y, z, c))
        })
        .unwrap();
    let inside: Vec<LidarPoint> = points
        .iter()
        .filter(|p| p.x >= lo[0] && p.x < hi[0] && p.y >= lo[1] && p.y < hi[1])
        .copied()
        .collect();
    assert_eq!(mm(&rect), mm(&inside));
    assert!(stats.skipped > 0, "nodes away from the rectangle not read");
    let corridor = Corridor::new(
        &[vec![[2_600_020.0, 1_200_010.0], [2_600_140.0, 1_200_250.0]]],
        4.0,
    );
    let tile = dir.join("tile.laz");
    {
        let mut b = Builder::from((1, 4));
        b.point_format.is_compressed = true;
        for t in [
            &mut b.transforms.x,
            &mut b.transforms.y,
            &mut b.transforms.z,
        ] {
            t.scale = 0.001;
        }
        b.transforms.x.offset = 2_600_000.0;
        b.transforms.y.offset = 1_200_000.0;
        let mut w = Writer::from_path(&tile, b.into_header().unwrap()).unwrap();
        for p in &points {
            w.write_point(Point {
                x: p.x,
                y: p.y,
                z: p.z,
                classification: Classification::new(p.class).unwrap(),
                return_number: 1,
                number_of_returns: 1,
                ..Default::default()
            })
            .unwrap();
        }
        w.close().unwrap();
    }
    let from_tiles = TileSource {
        tiles: vec![tile],
        scheme: None,
    };
    let a = collect(&source, Some(&corridor), Some(&[2, 14]));
    assert!(!a.is_empty());
    assert_eq!(a, collect(&from_tiles, Some(&corridor), Some(&[2, 14])));
    // The root alone: a thinned sample of the whole cloud.
    let mut root = 0;
    source
        .visit_rect(
            [f64::NEG_INFINITY; 2],
            [f64::INFINITY; 2],
            Some(0),
            None,
            &Budget::new(),
            &mut |_, _, _, _| root += 1,
        )
        .unwrap();
    assert_eq!(root, f.nodes[0].points);
    assert!(root < f.points / 2);
    // A COPC file is a LAZ file: it imports as one.
    let db = dir.join("p.ohl");
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_tiles(
            "copc",
            std::slice::from_ref(&path),
            "",
            None,
            DEFAULT_CHUNK,
            0.001,
            &Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    assert_eq!(cloud.points, points.len() as u64);
    drop(store);
    assert_eq!(
        collect(&CloudSource::open(&db, None).unwrap(), None, None),
        collect(&source, None, None)
    );
    // A hierarchy in two pages (as large files have): the root page
    // pointing to a page with the other nodes.
    let mut bytes = std::fs::read(&path).unwrap();
    let info = 375 + 54;
    let word = |b: &[u8], at: usize| u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
    let (root_at, size) = (
        word(&bytes, info + 40) as usize,
        word(&bytes, info + 48) as usize,
    );
    let page = bytes[root_at..root_at + size].to_vec();
    let (root, rest) = page.split_at(32);
    let child_at = bytes.len() as u64;
    bytes.extend_from_slice(rest);
    let mut pointer = rest[..32].to_vec();
    pointer[16..24].copy_from_slice(&child_at.to_le_bytes());
    pointer[24..28].copy_from_slice(&(rest.len() as i32).to_le_bytes());
    pointer[28..32].copy_from_slice(&(-1i32).to_le_bytes());
    let new_root = bytes.len() as u64;
    bytes.extend_from_slice(root);
    bytes.extend_from_slice(&pointer);
    bytes[info + 40..info + 48].copy_from_slice(&new_root.to_le_bytes());
    bytes[info + 48..info + 56].copy_from_slice(&64u64.to_le_bytes());
    let paged = dir.join("paged.copc.laz");
    std::fs::write(&paged, bytes).unwrap();
    let paged = copc::CopcSource::open(&paged).unwrap();
    assert_eq!(paged.file.nodes, f.nodes);
    assert_eq!(collect(&paged, None, None), collect(&source, None, None));
    // Opened by spec, either kind.
    let spec = open_source(&path.display().to_string()).unwrap();
    assert!(spec.describe().contains("COPC"));
    let spec = open_source(&format!("{}#copc", db.display())).unwrap();
    assert_eq!(spec.points(), Some(points.len() as u64));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn levels_of_detail_are_stored_and_read() {
    let dir = scratch("lod");
    let db = dir.join("p.ohl");
    let points = synthetic(2_600_000.0, 200, 100);
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_points(
            "s",
            "",
            None,
            DEFAULT_CHUNK,
            0.001,
            points.iter().copied(),
            &Budget::new(),
        )
        .unwrap();
    assert_eq!(cloud.points, points.len() as u64);
    assert!(store.has_lod(&cloud).unwrap());
    assert!(cloud.bounds[1][2] >= 420.0, "top known: {:?}", cloud.bounds);
    let blocks = store.lod_blocks(&cloud).unwrap();
    assert!(blocks.iter().all(|b| b.levels.is_some() && b.z1.is_some()));
    let total = |k: usize| -> u64 {
        blocks
            .iter()
            .map(|b| {
                if k == 0 {
                    b.count
                } else {
                    b.levels.unwrap()[k - 1]
                }
            })
            .sum()
    };
    for k in 1..=LOD_LEVELS as usize {
        let share = total(k) as f64 / total(0) as f64;
        let want = 0.25f64.powi(k as i32);
        assert!((share / want - 1.0).abs() < 0.1, "level {k}: {share}");
    }
    let read = |store: &Store, level: u8, whole: &[u8]| {
        let mut out = Vec::new();
        store
            .visit_level(
                &cloud,
                [i64::MIN, i64::MIN],
                [i64::MAX, i64::MAX],
                level,
                None,
                whole,
                &Budget::new(),
                &mut |x, y, z, c| out.push(point(x, y, z, c)),
            )
            .unwrap();
        mm(&out)
    };
    let full = read(&store, 0, &[]);
    assert_eq!(full, mm(&points));
    let stored: Vec<_> = (1..=LOD_LEVELS).map(|k| read(&store, k, &[])).collect();
    for (k, s) in stored.iter().enumerate() {
        assert_eq!(s.len() as u64, total(k + 1));
        // Nested: each level within the finer ones.
        let finer = if k == 0 { &full } else { &stored[k - 1] };
        assert!(s.iter().all(|p| finer.binary_search(p).is_ok()));
    }
    // A class read whole at any level.
    let wires = full.iter().filter(|p| p[3] == 14).count();
    let coarse_with_wires = read(&store, 3, &[14]);
    assert_eq!(
        coarse_with_wires.iter().filter(|p| p[3] == 14).count(),
        wires
    );
    // A version 1 file (no levels stored): the same points picked from
    // the whole blocks; building the levels makes them stored again.
    store
        .conn
        .execute("DELETE FROM pointcloud_lod", [])
        .unwrap();
    store
        .conn
        .execute(
            "UPDATE ols_meta SET value = '1' WHERE key = 'pointcloud_schema'",
            [],
        )
        .unwrap();
    assert!(!store.has_lod(&cloud).unwrap());
    assert!(
        store
            .lod_blocks(&cloud)
            .unwrap()
            .iter()
            .all(|b| b.levels.is_none())
    );
    for k in 1..=LOD_LEVELS {
        assert_eq!(read(&store, k, &[]), stored[k as usize - 1], "level {k}");
    }
    drop(store);
    let mut store = Store::open(&db).unwrap();
    let built = store.build_lod(&cloud, &Budget::new(), |_, _| {}).unwrap();
    assert_eq!(built, blocks.len());
    assert!(store.has_lod(&cloud).unwrap());
    let version: String = store
        .conn
        .query_row(
            "SELECT value FROM ols_meta WHERE key = 'pointcloud_schema'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION.to_string());
    assert_eq!(read(&store, 2, &[]), stored[1]);
    // Merging more points into a block replaces its levels.
    let more = synthetic(2_600_000.0, 2, 2);
    store
        .import_points(
            "s",
            "",
            None,
            DEFAULT_CHUNK,
            0.001,
            more.iter().copied(),
            &Budget::new(),
        )
        .unwrap();
    assert!(store.has_lod(&cloud).unwrap());
    // A share of the cloud, from the levels.
    drop(store);
    let source = CloudSource::open(&db, None).unwrap();
    let mut n = 0u64;
    let stats = source
        .visit_sample(0.02, &[], &Budget::new(), &mut |_, _, _, _| n += 1)
        .unwrap();
    let all = (points.len() + more.len()) as f64;
    assert!((n as f64 / all / 0.02 - 1.0).abs() < 0.15, "{n}");
    assert!(stats.points_read < (all / 10.0) as u64, "level 2 read only");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn nodes_read_by_level_of_detail() {
    use crate::octree::{LodReader, Refine};
    let dir = scratch("nodes");
    let db = dir.join("p.ohl");
    let points = synthetic(2_600_000.0, 300, 120);
    let mut store = Store::open(&db).unwrap();
    store
        .import_points(
            "s",
            "",
            None,
            DEFAULT_CHUNK,
            0.001,
            points.iter().copied(),
            &Budget::new(),
        )
        .unwrap();
    drop(store);
    let copc_path = dir.join("s.copc.laz");
    copc::write_copc(&copc_path, &points, 0.001, &Budget::new()).unwrap();
    for spec in [db.display().to_string(), copc_path.display().to_string()] {
        let mut reader = octree::open_lod(&spec).unwrap();
        let h = reader.hierarchy().clone();
        assert_eq!(h.points(), points.len() as u64, "{spec}");
        // The finest nodes (Replace) or all of them (Add): every point.
        let nodes: Vec<usize> = (0..h.nodes.len())
            .filter(|&i| h.refine == Refine::Add || h.nodes[i].children.is_empty())
            .collect();
        let mut other = reader.another().unwrap();
        let mut got = Vec::new();
        for (k, &i) in nodes.iter().enumerate() {
            let r: &mut Box<dyn LodReader> = if k % 2 == 0 { &mut reader } else { &mut other };
            let b = h.nodes[i].bounds;
            let stats = r
                .load(i, None, &[], &Budget::new(), &mut |x, y, z, c| {
                    assert!(x >= b[0][0] && x <= b[1][0], "{spec}");
                    assert!(z >= b[0][2] - 1e-6 && z <= b[1][2] + 1e-6, "{spec}");
                    got.push(point(x, y, z, c));
                })
                .unwrap();
            assert_eq!(stats.points_visited, h.nodes[i].points, "{spec} node {i}");
        }
        assert_eq!(mm(&got), mm(&points), "{spec}");
        assert!(!reader.describe().is_empty());
    }
    std::fs::remove_dir_all(dir).ok();
}

/// Loading speed of a large synthetic cloud (release: `cargo test
/// --release -p ohl-pointcloud -- --ignored --nocapture lod_speed`):
/// 25 million points imported, levels built, then one view's selection
/// under a 5 million point budget read by node.
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn lod_speed() {
    use crate::octree::{LodLimits, LodView};
    use std::time::Instant;
    let dir = scratch("speed");
    let db = dir.join("p.ohl");
    std::fs::remove_file(&db).ok();
    // 2.5 km × 1.25 km at 0.5 m: 12.5 million points, twice (two strips).
    let mut store = Store::open(&db).unwrap();
    let t = Instant::now();
    for strip in 0..2 {
        let pts = synthetic(2_600_000.0 + 2500.0 * f64::from(strip), 2500, 1250);
        store
            .import_points("s", "", None, DEFAULT_CHUNK, 0.001, pts, &Budget::new())
            .unwrap();
    }
    let cloud = store.cloud(None).unwrap();
    println!(
        "import + levels: {} points in {:.1?}",
        cloud.points,
        t.elapsed()
    );
    drop(store);
    let size = std::fs::metadata(&db).map_or(0, |m| m.len());
    println!("file: {:.0} MB", size as f64 / 1e6);
    let t = Instant::now();
    let mut reader = octree::open_lod(&db.display().to_string()).unwrap();
    let h = reader.hierarchy().clone();
    println!("hierarchy: {} nodes in {:.1?}", h.nodes.len(), t.elapsed());
    let eye = [2_602_500.0, 1_200_600.0, 700.0];
    let distance = |b: &octree::Box3| {
        let d = [0, 1, 2].map(|k| (b[0][k] - eye[k]).max(eye[k] - b[1][k]).max(0.0));
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    let sees = |_: &octree::Box3| true;
    let t = Instant::now();
    let s = h.select(
        &LodView {
            distance: &distance,
            sees: &sees,
            pixels_per_radian: 1000.0 / 45f64.to_radians(),
            render_distance: 1e9,
        },
        &LodLimits {
            point_budget: 5_000_000,
            max_error: 1.0,
            share: 1.0,
        },
    );
    println!(
        "selection: {} nodes ({} drawn), {} points, in {:.2?}",
        s.nodes.len(),
        s.draw.len(),
        s.points,
        t.elapsed()
    );
    let t = Instant::now();
    let mut n = 0u64;
    for &i in &s.draw {
        reader
            .load(i, None, &[], &Budget::new(), &mut |_, _, _, _| n += 1)
            .unwrap();
    }
    let dt = t.elapsed();
    println!(
        "read: {n} points in {dt:.2?} on one thread ({:.1} M points/s)",
        n as f64 / dt.as_secs_f64() / 1e6
    );
    assert!(n <= 5_200_000);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_stored_cloud_exports_to_copc() {
    let dir = scratch("to-copc");
    let db = dir.join("p.ohl");
    let points = synthetic(2_600_000.0, 120, 60);
    let mut store = Store::open(&db).unwrap();
    let (cloud, _) = store
        .import_points(
            "s",
            "",
            None,
            DEFAULT_CHUNK,
            0.001,
            points.iter().copied(),
            &Budget::new(),
        )
        .unwrap();
    let out = dir.join("s.copc.laz");
    let written = store
        .export_copc(&cloud, &out, None, None, &Budget::new())
        .unwrap();
    assert_eq!(written.points, points.len() as u64);
    let copc = copc::CopcSource::open(&out).unwrap();
    let mut back = Vec::new();
    copc.visit_records(None, None, &Budget::new(), &mut |p| back.push(*p))
        .unwrap();
    assert_eq!(mm(&back), mm(&points));
    let key = |p: &LidarPoint| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    let by: HashMap<_, _> = points.iter().map(|p| (key(p), p)).collect();
    assert!(back.iter().all(|p| {
        let q = by[&key(p)];
        (p.intensity, p.number_of_returns) == (q.intensity, q.number_of_returns)
    }));
    // Only some classes, only an area.
    let area = ([2_600_010.0, 1_200_010.0], [2_600_060.0, 1_200_030.0]);
    let part = dir.join("part.copc.laz");
    let written = store
        .export_copc(&cloud, &part, Some(&[5]), Some(area), &Budget::new())
        .unwrap();
    let want = points
        .iter()
        .filter(|p| {
            p.class == 5
                && p.x >= area.0[0]
                && p.x < area.1[0]
                && p.y >= area.0[1]
                && p.y < area.1[1]
        })
        .count();
    assert_eq!(written.points, want as u64);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_cloud_moves_into_another_coordinate_system() {
    let dir = scratch("reproject");
    let db = dir.join("p.ohl");
    std::fs::remove_file(&db).ok();
    let mut store = Store::open(&db).unwrap();
    let pts = synthetic(2_600_000.0, 100, 60);
    let budget = Budget::new();
    store
        .import_points(
            "lidar",
            "EPSG:2056",
            Some("scheme"),
            DEFAULT_CHUNK,
            0.001,
            pts,
            &budget,
        )
        .unwrap();
    let before = store.cloud(Some("lidar")).unwrap();
    let mut old = Vec::new();
    store
        .visit(&before, None, None, &budget, &mut |x, y, z, c| {
            old.push([
                (x * 1000.0).round() as i64,
                (y * 1000.0).round() as i64,
                (z * 1000.0).round() as i64,
                i64::from(c),
            ])
        })
        .unwrap();
    // A shift that crosses chunk borders.
    let shift = |p: &mut [[f64; 3]]| -> Result<(), String> {
        for q in p {
            q[0] -= 2_000_000.0 - 20.25;
            q[1] += 3.5;
        }
        Ok(())
    };
    let mut calls = 0;
    let after = store
        .reproject_cloud(&before, "EPSG:32632", &shift, &budget, |_, _| calls += 1)
        .unwrap();
    assert!(calls > 0);
    assert_eq!(after.name, "lidar");
    assert_eq!(after.crs, "EPSG:32632");
    assert_eq!(after.class_scheme.as_deref(), Some("scheme"));
    assert_eq!(after.points, before.points);
    assert_eq!(store.clouds().unwrap().len(), 1);
    assert!(store.has_lod(&after).unwrap());
    assert_eq!(store.class_counts(&after).unwrap(), store_counts(&old));
    let mut new = Vec::new();
    store
        .visit(&after, None, None, &budget, &mut |x, y, z, c| {
            new.push([
                (x * 1000.0).round() as i64,
                (y * 1000.0).round() as i64,
                (z * 1000.0).round() as i64,
                i64::from(c),
            ])
        })
        .unwrap();
    let moved: Vec<[i64; 4]> = old
        .iter()
        .map(|p| [p[0] - 1_999_979_750, p[1] + 3500, p[2], p[3]])
        .collect();
    assert_eq!(sorted(new), sorted(moved));
    // A failing transformation keeps the old cloud.
    let fail = |_: &mut [[f64; 3]]| -> Result<(), String> { Err("no image".into()) };
    assert!(
        store
            .reproject_cloud(&after, "EPSG:4326", &fail, &budget, |_, _| {})
            .is_err()
    );
    assert_eq!(store.clouds().unwrap(), vec![after]);
}

fn store_counts(points: &[[i64; 4]]) -> Vec<(u8, u64)> {
    let mut m = std::collections::BTreeMap::new();
    for p in points {
        *m.entry(p[3] as u8).or_insert(0u64) += 1;
    }
    m.into_iter().collect()
}

/// Every stored row of a cloud by what it is (not by row id): blocks by
/// (cx, cy, class), their levels, the cloud's summary.
type Snapshot = (
    Vec<(
        i64,
        i64,
        i64,
        i64,
        i64,
        Vec<u8>,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
    )>,
    Vec<(i64, i64, i64, i64, i64, i64, Vec<u8>)>,
    CloudInfo,
);

fn snapshot(path: &Path, name: &str) -> Snapshot {
    let conn = Connection::open(path).unwrap();
    let blocks = conn
        .prepare(
            "SELECT cx, cy, class, count, z0, xyz, intensity, returns FROM pointcloud_block
             ORDER BY cx, cy, class",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let lods = conn
        .prepare(
            "SELECT b.cx, b.cy, b.class, l.level, l.count, l.z1, l.xyz FROM pointcloud_lod l
             JOIN pointcloud_block b ON b.id = l.block ORDER BY b.cx, b.cy, b.class, l.level",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut cloud = Store::open(path).unwrap().cloud(Some(name)).unwrap();
    cloud.id = 0;
    (blocks, lods, cloud)
}

/// Single points changed — a height and class within its chunk, one
/// moved to another chunk and a class of its own (a new block), the only
/// point of a block moved out (the block goes) — read back exactly, the
/// counts and levels follow; the inverses give every stored byte back.
#[test]
fn changed_points_read_back_and_undo_gives_every_byte_back() {
    use crate::edit::{PointChange, StoredPoint};
    let dir = scratch("edit");
    let db = dir.join("p.ohl");
    std::fs::remove_file(&db).ok();
    let x0 = 2_600_000.0;
    let points = synthetic(x0, 100, 30);
    let mut store = Store::open(&db).unwrap();
    store
        .import_points("c", "", None, 64.0, 0.001, points.clone(), &Budget::new())
        .unwrap();
    let before = snapshot(&db, "c");
    let cloud = store.cloud(Some("c")).unwrap();
    // A ground point (class 2) at x0 + 10.5, y + 3.5.
    let at = [x0 + 10.5, 1_200_003.5, 0.0];
    let ground = points
        .iter()
        .find(|p| p.x == at[0] && p.y == at[1])
        .unwrap();
    assert_eq!(ground.class, 2);
    let (a, ia) = store
        .find_point("c", [ground.x, ground.y, ground.z], Some(2), 0.01)
        .unwrap()
        .unwrap();
    assert_eq!(
        a.position(&cloud),
        [ground.x, ground.y, ground.z].map(|v| (v * 1000.0).round() / 1000.0)
    );
    // A wire point (class 14, every 997th) to move into another chunk.
    let wire = points
        .iter()
        .find(|p| p.class == 14 && p.x < x0 + 60.0)
        .unwrap();
    let (b, _) = store
        .find_point("c", [wire.x, wire.y, wire.z], None, 0.01)
        .unwrap()
        .unwrap();
    assert_eq!(b.class, 14);
    let a2 = StoredPoint {
        xyz: [a.xyz[0], a.xyz[1], a.xyz[2] + 1234],
        class: 5,
        ..a
    };
    let b2 = StoredPoint {
        xyz: [b.xyz[0] + 140_000, b.xyz[1], b.xyz[2] - 5],
        class: 31,
        ..b
    };
    let changes = [
        PointChange {
            from: a,
            from_index: Some(ia),
            to: a2,
            to_index: None,
        },
        PointChange {
            from: b,
            from_index: None,
            to: b2,
            to_index: None,
        },
    ];
    let applied = store.change_points("c", &changes).unwrap();
    // Read back: the new points there, the old ones gone, as many points.
    let all = collect(&CloudSource::open(&db, Some("c")).unwrap(), None, None);
    let mm = |p: [f64; 3], c: u8| {
        let m = |v: f64| (v * 1000.0).round() as i64;
        [m(p[0]), m(p[1]), m(p[2]), i64::from(c)]
    };
    for (p, c, present) in [
        (a2.position(&cloud), 5, true),
        (b2.position(&cloud), 31, true),
        (a.position(&cloud), 2, false),
        (b.position(&cloud), 14, false),
    ] {
        assert_eq!(all.contains(&mm(p, c)), present, "{p:?} class {c}");
    }
    assert_eq!(all.len(), points.len());
    let after = store.cloud(Some("c")).unwrap();
    assert_eq!(after.points, cloud.points);
    // The class of its own, counted; the cloud reaches the new chunk.
    let counts = store.class_counts(&after).unwrap();
    assert!(counts.contains(&(31, 1)), "{counts:?}");
    assert!(after.bounds[1][0] > cloud.bounds[1][0]);
    assert!(store.has_lod(&after).unwrap());
    // Exactly as the changed point: found again by its coordinates.
    let (found, _) = store
        .find_point("c", a2.position(&cloud), None, 0.0005)
        .unwrap()
        .unwrap();
    assert_eq!(found, a2);
    // The only point of block 31 moved back out: the block goes.
    let back: Vec<PointChange> = applied.iter().rev().map(PointChange::inverse).collect();
    store.change_points("c", &back).unwrap();
    assert_eq!(snapshot(&db, "c"), before, "undo gives every byte back");
    // A point that is not there: refused, nothing written.
    let missing = PointChange {
        from: a2,
        from_index: None,
        to: a,
        to_index: None,
    };
    assert!(store.change_points("c", &[missing]).is_err());
    assert_eq!(snapshot(&db, "c"), before);
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}
