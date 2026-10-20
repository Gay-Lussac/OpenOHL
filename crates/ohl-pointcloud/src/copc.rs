// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! COPC — Cloud Optimized Point Cloud (copc.io): a LAS 1.4 LAZ file whose
//! points are grouped by the nodes of an octree, each node one LAZ chunk,
//! with the octree (the *hierarchy*: per node its key, byte range and
//! point count) in an extended VLR. A node holds a thinned sample of its
//! cube; its eight children add detail (additive refinement). Reading
//! the nodes down to a depth gives the whole cloud at that density.
//!
//! - [`CopcFile`]: header, COPC info and hierarchy; decodes single nodes
//!   (on top of the `laz` crate's chunk decompression).
//! - [`CopcSource`]: a [`PointSource`] reading only the nodes of an area,
//!   down to a depth if asked.
//! - [`write_copc`]: points to a COPC file (in memory: for exports of up
//!   to some 10⁸ points).
//!
//! Point formats 6, 7 and 8 (what COPC allows) are read; points written
//! are format 6 (coordinates, intensity, returns, class).

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use laz::record::{
    LayeredPointRecordCompressor, LayeredPointRecordDecompressor, RecordCompressor,
    RecordDecompressor,
};
use ohl_core::budget::Budget;
use ohl_lidar::{Corridor, LidarPoint};

use crate::{PointSource, Stats};

/// Size of the LAS 1.4 header, bytes.
const HEADER_SIZE: usize = 375;
/// Size of a VLR header, bytes.
const VLR_HEADER: usize = 54;
/// Size of an extended VLR header, bytes.
const EVLR_HEADER: usize = 60;
/// Size of the COPC info VLR's payload, bytes.
const INFO_SIZE: usize = 160;
/// Size of a hierarchy entry, bytes.
const ENTRY_SIZE: usize = 32;
/// Points of a node above which the writer splits it.
const MAX_NODE_POINTS: usize = 100_000;
/// Cells per side of the sampling grid of a node (the writer keeps one
/// point per cell in a node, the rest go to its children).
const GRID: u32 = 128;
/// Deepest node written.
const MAX_DEPTH: i32 = 20;
/// Hierarchy pages read at most (a damaged file cannot loop).
const MAX_PAGES: usize = 1_000_000;
/// Points a node may claim per byte of its LAZ chunk: far above what LAZ
/// reaches (a few bytes per point), so only a damaged or crafted
/// hierarchy is refused.
const MAX_POINTS_PER_BYTE: u64 = 1024;
/// Points reserved at most up front when a node is decoded (the vector
/// grows past it as points actually come).
const MAX_RESERVE: u64 = 1 << 20;
/// Bytes of a point record decoded at most (LAS 1.4 formats 6 to 8 with
/// extra bytes stay well below).
const MAX_ITEMS_SIZE: u64 = 1024;

/// The COPC info VLR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopcInfo {
    /// Centre of the root cube (x, y, z).
    pub center: [f64; 3],
    /// Half the side of the root cube, m.
    pub halfsize: f64,
    /// Distance between points at the root, m (halved at every level).
    pub spacing: f64,
    /// File offset of the root hierarchy page.
    pub root_hier_offset: u64,
    /// Size of the root hierarchy page, bytes.
    pub root_hier_size: u64,
    /// GPS time range of the points.
    pub gpstime: [f64; 2],
}

/// Key of an octree node: depth (0: the root) and position at that depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoxelKey {
    /// Depth.
    pub depth: i32,
    /// Position along x, 0 … 2^depth − 1.
    pub x: i32,
    /// Position along y.
    pub y: i32,
    /// Position along z.
    pub z: i32,
}

impl VoxelKey {
    /// The root.
    pub const ROOT: Self = Self {
        depth: 0,
        x: 0,
        y: 0,
        z: 0,
    };

    /// Child `octant` (bit 0: x, bit 1: y, bit 2: z).
    pub fn child(&self, octant: u8) -> Self {
        Self {
            depth: self.depth + 1,
            x: 2 * self.x + i32::from(octant & 1),
            y: 2 * self.y + i32::from((octant >> 1) & 1),
            z: 2 * self.z + i32::from((octant >> 2) & 1),
        }
    }

    /// The parent (`None` for the root).
    pub fn parent(&self) -> Option<Self> {
        (self.depth > 0).then(|| Self {
            depth: self.depth - 1,
            x: self.x >> 1,
            y: self.y >> 1,
            z: self.z >> 1,
        })
    }
}

/// A node with points: its key, where its LAZ chunk lies, its points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopcNode {
    /// Key.
    pub key: VoxelKey,
    /// File offset of its chunk.
    pub offset: u64,
    /// Size of its chunk, bytes.
    pub byte_size: u64,
    /// Points.
    pub points: u64,
}

/// A COPC file: header, info and hierarchy (the points are read by node).
#[derive(Debug, Clone)]
pub struct CopcFile {
    /// The file.
    pub path: PathBuf,
    /// COPC info.
    pub info: CopcInfo,
    /// Coordinate scales (x, y, z).
    pub scale: [f64; 3],
    /// Coordinate offsets (x, y, z).
    pub offset: [f64; 3],
    /// Bounds of the points from the header: minimum and maximum.
    pub bounds: [[f64; 3]; 2],
    /// Points in the file.
    pub points: u64,
    /// LAS point format (6, 7 or 8).
    pub point_format: u8,
    /// Bytes per point record.
    pub record_length: u16,
    /// Nodes with points, by depth then key.
    pub nodes: Vec<CopcNode>,
    laz: laz::LazVlr,
}

fn f64_at(b: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().expect("2 bytes"))
}

/// A VLR's user id (16 bytes, NUL padded) as text.
fn user_id(b: &[u8]) -> &[u8] {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    &b[..end]
}

/// Whether `path` is a COPC file: a LAS 1.4 file whose first VLR is the
/// COPC info. Reads its first 429 bytes.
pub fn is_copc(path: &Path) -> bool {
    let mut head = vec![0u8; HEADER_SIZE + VLR_HEADER];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head[0..4] == b"LASF"
        && head[24] == 1
        && head[25] == 4
        && u16_at(&head, 94) as usize == HEADER_SIZE
        && user_id(&head[HEADER_SIZE + 2..HEADER_SIZE + 18]) == b"copc"
        && u16_at(&head, HEADER_SIZE + 18) == 1
}

impl CopcFile {
    /// Reads the header, the COPC info and the whole hierarchy.
    ///
    /// Cost: the header, the VLRs and the hierarchy pages (32 bytes per
    /// node: ~1 ms per 10⁴ nodes); no point is read.
    pub fn open(path: &Path) -> Result<Self, String> {
        let e = |m: String| format!("{}: {m}", path.display());
        let mut f = BufReader::new(File::open(path).map_err(|x| e(x.to_string()))?);
        let file_len = f.get_ref().metadata().map_err(|x| e(x.to_string()))?.len();
        let mut h = vec![0u8; HEADER_SIZE];
        f.read_exact(&mut h).map_err(|x| e(x.to_string()))?;
        if &h[0..4] != b"LASF" || h[24] != 1 || h[25] != 4 {
            return Err(e("not a LAS 1.4 file (COPC needs LAS 1.4)".into()));
        }
        let header_size = u16_at(&h, 94) as u64;
        let n_vlrs = u32_at(&h, 100);
        let point_format = h[104] & 0x3f;
        let record_length = u16_at(&h, 105);
        if !(6..=8).contains(&point_format) {
            return Err(e(format!(
                "point format {point_format}: COPC needs 6, 7 or 8"
            )));
        }
        let scale = [f64_at(&h, 131), f64_at(&h, 139), f64_at(&h, 147)];
        let offset = [f64_at(&h, 155), f64_at(&h, 163), f64_at(&h, 171)];
        let bounds = [
            [f64_at(&h, 187), f64_at(&h, 203), f64_at(&h, 219)],
            [f64_at(&h, 179), f64_at(&h, 195), f64_at(&h, 211)],
        ];
        let points = u64_at(&h, 247);
        // VLRs: the COPC info (first) and the LAZ items.
        let mut info = None;
        let mut laz = None;
        let mut at = header_size;
        for _ in 0..n_vlrs {
            f.seek(SeekFrom::Start(at)).map_err(|x| e(x.to_string()))?;
            let mut vh = [0u8; VLR_HEADER];
            f.read_exact(&mut vh).map_err(|x| e(x.to_string()))?;
            let uid = user_id(&vh[2..18]).to_vec();
            let record = u16_at(&vh, 18);
            let len = u16_at(&vh, 20) as usize;
            let mut data = vec![0u8; len];
            f.read_exact(&mut data).map_err(|x| e(x.to_string()))?;
            match (uid.as_slice(), record) {
                (b"copc", 1) if len >= INFO_SIZE => {
                    info = Some(CopcInfo {
                        center: [f64_at(&data, 0), f64_at(&data, 8), f64_at(&data, 16)],
                        halfsize: f64_at(&data, 24),
                        spacing: f64_at(&data, 32),
                        root_hier_offset: u64_at(&data, 40),
                        root_hier_size: u64_at(&data, 48),
                        gpstime: [f64_at(&data, 56), f64_at(&data, 64)],
                    });
                }
                (b"laszip encoded", 22204) => {
                    laz = Some(laz::LazVlr::from_buffer(&data).map_err(|x| e(x.to_string()))?);
                }
                _ => {}
            }
            at += (VLR_HEADER + len) as u64;
        }
        let info = info.ok_or_else(|| e("no COPC info VLR: not a COPC file".into()))?;
        let laz = laz.ok_or_else(|| e("no LAZ VLR: COPC points are LAZ compressed".into()))?;
        // The hierarchy: the root page, and the pages it points to.
        let mut nodes = Vec::new();
        let mut pages = vec![(info.root_hier_offset, info.root_hier_size)];
        let mut read = 0;
        while let Some((page_at, size)) = pages.pop() {
            read += 1;
            if read > MAX_PAGES || size > 1 << 31 || size > file_len {
                return Err(e("hierarchy too large or damaged".into()));
            }
            f.seek(SeekFrom::Start(page_at))
                .map_err(|x| e(x.to_string()))?;
            let mut page = vec![0u8; size as usize];
            f.read_exact(&mut page).map_err(|x| e(x.to_string()))?;
            for entry in page.as_chunks::<ENTRY_SIZE>().0 {
                let key = VoxelKey {
                    depth: i32_at(entry, 0),
                    x: i32_at(entry, 4),
                    y: i32_at(entry, 8),
                    z: i32_at(entry, 12),
                };
                let offset = u64_at(entry, 16);
                let byte_size = i32_at(entry, 24);
                let count = i32_at(entry, 28);
                match count {
                    -1 => pages.push((offset, byte_size.max(0) as u64)),
                    0 => {}
                    n if n > 0 => {
                        let byte_size = u64::try_from(byte_size).unwrap_or(0);
                        if n as u64 > byte_size.saturating_mul(MAX_POINTS_PER_BYTE) {
                            return Err(e(format!(
                                "hierarchy entry {key:?}: {n} points in {byte_size} bytes"
                            )));
                        }
                        nodes.push(CopcNode {
                            key,
                            offset,
                            byte_size,
                            points: n as u64,
                        });
                    }
                    _ => return Err(e(format!("hierarchy entry {key:?}: {count} points"))),
                }
            }
        }
        nodes.sort_by_key(|n| n.key);
        Ok(Self {
            path: path.to_path_buf(),
            info,
            scale,
            offset,
            bounds,
            points,
            point_format,
            record_length,
            nodes,
            laz,
        })
    }

    /// The cube of a node: minimum and maximum corner.
    pub fn node_bounds(&self, key: &VoxelKey) -> [[f64; 3]; 2] {
        let side = 2.0 * self.info.halfsize / f64::from(1u32 << key.depth.clamp(0, 30));
        let lo = [0, 1, 2].map(|k| self.info.center[k] - self.info.halfsize);
        let at = [key.x, key.y, key.z];
        let min = [0, 1, 2].map(|k| lo[k] + f64::from(at[k]) * side);
        [min, min.map(|v| v + side)]
    }

    /// Distance between the points of a node at `depth`, m.
    pub fn spacing(&self, depth: i32) -> f64 {
        self.info.spacing / f64::from(1u32 << depth.clamp(0, 30))
    }

    /// The deepest level with points.
    pub fn depth(&self) -> i32 {
        self.nodes.iter().map(|n| n.key.depth).max().unwrap_or(0)
    }

    /// Nodes and points per depth.
    pub fn levels(&self) -> Vec<(i32, usize, u64)> {
        let mut out: Vec<(i32, usize, u64)> = Vec::new();
        for n in &self.nodes {
            match out.last_mut() {
                Some(l) if l.0 == n.key.depth => {
                    l.1 += 1;
                    l.2 += n.points;
                }
                _ => out.push((n.key.depth, 1, n.points)),
            }
        }
        out
    }

    /// Opens the file for [`CopcFile::read_node`].
    pub fn reader(&self) -> Result<BufReader<File>, String> {
        File::open(&self.path)
            .map(BufReader::new)
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// Decodes the points of `node` (withheld points left out), calling
    /// `visit` for each.
    ///
    /// Cost: LAZ decompression, ~0.1–0.2 µs per point.
    pub fn read_node(
        &self,
        file: &mut BufReader<File>,
        node: &CopcNode,
        visit: &mut dyn FnMut(&LidarPoint),
    ) -> Result<(), String> {
        let e = |m: String| format!("{} node {:?}: {m}", self.path.display(), node.key);
        file.seek(SeekFrom::Start(node.offset))
            .map_err(|x| e(x.to_string()))?;
        let mut dec = LayeredPointRecordDecompressor::new(&mut *file);
        dec.set_fields_from(self.laz.items())
            .map_err(|x| e(x.to_string()))?;
        let size = self.laz.items_size();
        if size < 30 {
            return Err(e("point records too short".into()));
        }
        if size > MAX_ITEMS_SIZE {
            return Err(e(format!("point records of {size} bytes: damaged?")));
        }
        let mut buf = vec![0u8; size as usize];
        for _ in 0..node.points {
            dec.decompress_next(&mut buf)
                .map_err(|x| e(x.to_string()))?;
            // Withheld (classification flag 3): deleted by the surveyor.
            if buf[15] & 0x08 != 0 {
                continue;
            }
            let c = |k: usize| f64::from(i32_at(&buf, 4 * k)) * self.scale[k] + self.offset[k];
            visit(&LidarPoint {
                x: c(0),
                y: c(1),
                z: c(2),
                intensity: u16_at(&buf, 12),
                return_number: buf[14] & 0x0f,
                number_of_returns: buf[14] >> 4,
                class: buf[16],
            });
        }
        Ok(())
    }

    /// Decodes the nodes `ids` (indices into [`CopcFile::nodes`]) in
    /// parallel batches of about 4 million points; `visit` gets each point
    /// and says whether it took it. Exit clause: `run`, checked per batch.
    fn decode(
        &self,
        ids: &[usize],
        run: &ohl_core::budget::Run<'_>,
        stats: &mut Stats,
        visit: &mut dyn FnMut(&LidarPoint) -> bool,
    ) -> Result<(), String> {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let mut start = 0;
        while start < ids.len() {
            run.check().map_err(|stop| format!("stopped: {stop}"))?;
            let mut end = start;
            let mut points = 0;
            while end < ids.len() && (points < 4_000_000 || end == start) {
                points += self.nodes[ids[end]].points;
                end += 1;
            }
            let batch = &ids[start..end];
            let per = batch.len().div_ceil(threads);
            let decoded: Vec<Result<Vec<LidarPoint>, String>> = std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .chunks(per)
                    .map(|part| {
                        scope.spawn(move || {
                            let mut file = match self.reader() {
                                Ok(f) => f,
                                Err(e) => return vec![Err(e)],
                            };
                            part.iter()
                                .map(|&i| {
                                    let node = &self.nodes[i];
                                    let reserve = node.points.min(MAX_RESERVE) as usize;
                                    let mut out = Vec::with_capacity(reserve);
                                    self.read_node(&mut file, node, &mut |p| out.push(*p))?;
                                    Ok(out)
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|h| h.join().expect("decoder thread"))
                    .collect()
            });
            for points in decoded {
                let points = points?;
                stats.read += 1;
                stats.points_read += points.len() as u64;
                for p in &points {
                    if visit(p) {
                        stats.points_visited += 1;
                    }
                }
            }
            start = end;
        }
        Ok(())
    }

    /// The nodes (indices) down to `max_depth` (all without) whose plan
    /// square meets the rectangle `min`–`max`.
    pub fn nodes_in(&self, min: [f64; 2], max: [f64; 2], max_depth: Option<i32>) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|&i| {
                let n = &self.nodes[i];
                let b = self.node_bounds(&n.key);
                max_depth.is_none_or(|d| n.key.depth <= d)
                    && b[0][0] <= max[0]
                    && b[1][0] >= min[0]
                    && b[0][1] <= max[1]
                    && b[1][1] >= min[1]
            })
            .collect()
    }
}

/// A COPC file as a [`PointSource`]: queries read only the nodes of their
/// area (and depth).
#[derive(Debug, Clone)]
pub struct CopcSource {
    /// The file.
    pub file: CopcFile,
    /// Class scheme (`.ohlc` text): the file of that name next to it.
    pub scheme: Option<String>,
}

pub(crate) fn wanted_classes(classes: Option<&[u8]>) -> [bool; 256] {
    let mut wanted = [true; 256];
    if let Some(cs) = classes {
        wanted = [false; 256];
        for &c in cs {
            wanted[c as usize] = true;
        }
    }
    wanted
}

impl CopcSource {
    /// Opens a COPC file, with the class scheme of the `.ohlc` file of its
    /// name next to it (`survey.copc.laz` → `survey.copc.ohlc` or
    /// `survey.ohlc`), if any.
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = CopcFile::open(path)?;
        let ohlc = [
            path.with_extension(ohl_lidar::classes::CLASS_FILE_EXTENSION),
            path.with_extension("")
                .with_extension(ohl_lidar::classes::CLASS_FILE_EXTENSION),
        ];
        let scheme = ohlc.iter().find_map(|p| std::fs::read_to_string(p).ok());
        Ok(Self { file, scheme })
    }

    /// Calls `visit(x, y, z, class)` for the points in the rectangle
    /// `min`–`max` (lower edges in, upper out) of `classes` (all without),
    /// of the nodes down to `max_depth` (all without): the cloud at the
    /// density of that depth.
    pub fn visit_rect(
        &self,
        min: [f64; 2],
        max: [f64; 2],
        max_depth: Option<i32>,
        classes: Option<&[u8]>,
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String> {
        let run = budget.start();
        let wanted = wanted_classes(classes);
        let ids = self.file.nodes_in(min, max, max_depth);
        let mut stats = Stats {
            skipped: self.file.nodes.len() - ids.len(),
            ..Stats::default()
        };
        self.file.decode(&ids, &run, &mut stats, &mut |p| {
            let take = wanted[p.class as usize]
                && p.x >= min[0]
                && p.x < max[0]
                && p.y >= min[1]
                && p.y < max[1];
            if take {
                visit(p.x, p.y, p.z, p.class);
            }
            take
        })?;
        Ok(stats)
    }

    /// Calls `visit` for every point (all attributes) of the nodes down to
    /// `max_depth` (all without) inside `area` (all without).
    pub fn visit_records(
        &self,
        area: Option<&Corridor>,
        max_depth: Option<i32>,
        budget: &Budget,
        visit: &mut dyn FnMut(&LidarPoint),
    ) -> Result<Stats, String> {
        let run = budget.start();
        let (lo, hi) = match area {
            None => ([f64::NEG_INFINITY; 2], [f64::INFINITY; 2]),
            Some(a) => a.bounds().unwrap_or(([1.0; 2], [0.0; 2])),
        };
        let ids: Vec<usize> = self
            .file
            .nodes_in(lo, hi, max_depth)
            .into_iter()
            .filter(|&i| {
                let b = self.file.node_bounds(&self.file.nodes[i].key);
                area.is_none_or(|a| !a.local([b[0][0], b[0][1]], [b[1][0], b[1][1]]).is_empty())
            })
            .collect();
        let mut stats = Stats {
            skipped: self.file.nodes.len() - ids.len(),
            ..Stats::default()
        };
        self.file.decode(&ids, &run, &mut stats, &mut |p| {
            let take = area.is_none_or(|a| a.contains(p.x, p.y));
            if take {
                visit(p);
            }
            take
        })?;
        Ok(stats)
    }
}

impl PointSource for CopcSource {
    fn describe(&self) -> String {
        format!(
            "COPC file {} ({} points, {} nodes)",
            self.file.path.display(),
            self.file.points,
            self.file.nodes.len()
        )
    }

    fn class_scheme(&self) -> Option<String> {
        self.scheme.clone()
    }

    fn visit(
        &self,
        area: Option<&Corridor>,
        classes: Option<&[u8]>,
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String> {
        let wanted = wanted_classes(classes);
        let mut kept = 0u64;
        let mut stats = self.visit_records(area, None, budget, &mut |p| {
            if wanted[p.class as usize] {
                kept += 1;
                visit(p.x, p.y, p.z, p.class);
            }
        })?;
        stats.points_visited = kept;
        Ok(stats)
    }

    fn points(&self) -> Option<u64> {
        Some(self.file.points)
    }

    fn visit_sample(
        &self,
        share: f64,
        _whole: &[u8],
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String> {
        // The shallowest depth holding the share; every n-th of it.
        // (Classes cannot be read whole without reading every node.)
        let target = (self.file.points as f64 * share.clamp(0.0, 1.0)).ceil() as u64;
        let mut depth = 0;
        let mut sum = 0;
        for (d, _, n) in self.file.levels() {
            sum += n;
            depth = d;
            if sum >= target {
                break;
            }
        }
        let every = (sum as f64 / target.max(1) as f64).floor().max(1.0) as u64;
        let mut keep = crate::Every::new(every, &[]);
        let mut kept = 0;
        let mut stats = self.visit_rect(
            [f64::NEG_INFINITY; 2],
            [f64::INFINITY; 2],
            Some(depth),
            None,
            budget,
            &mut |x, y, z, c| {
                if keep.keep(c) {
                    kept += 1;
                    visit(x, y, z, c);
                }
            },
        )?;
        stats.points_visited = kept;
        Ok(stats)
    }
}

/// What [`write_copc`] wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct CopcWritten {
    /// Points.
    pub points: u64,
    /// Nodes with points.
    pub nodes: usize,
    /// Deepest level.
    pub depth: i32,
}

/// One point as a LAS point format 6 record.
fn record(p: &LidarPoint, scale: f64, offset: [f64; 3]) -> Result<[u8; 30], String> {
    let mut r = [0u8; 30];
    for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
        let q = ((v - offset[k]) / scale).round();
        if !(q >= f64::from(i32::MIN) && q <= f64::from(i32::MAX)) {
            return Err(format!(
                "coordinate {v} beyond the 32-bit range at scale {scale}"
            ));
        }
        r[4 * k..4 * k + 4].copy_from_slice(&(q as i32).to_le_bytes());
    }
    r[12..14].copy_from_slice(&p.intensity.to_le_bytes());
    r[14] = (p.return_number.clamp(1, 15)) | (p.number_of_returns.clamp(1, 15) << 4);
    r[16] = p.class;
    Ok(r)
}

/// Splits points into octree nodes: a node keeps one point per cell of a
/// [`GRID`]³ grid over its cube while it holds more than
/// [`MAX_NODE_POINTS`]; the others go to its children.
fn split(
    points: &[LidarPoint],
    center: [f64; 3],
    halfsize: f64,
    run: &ohl_core::budget::Run<'_>,
) -> Result<Vec<(VoxelKey, Vec<u32>)>, String> {
    let mut out = Vec::new();
    let mut stack = vec![(VoxelKey::ROOT, (0..points.len() as u32).collect::<Vec<_>>())];
    let lo = center.map(|c| c - halfsize);
    while let Some((key, ids)) = stack.pop() {
        run.check().map_err(|stop| format!("stopped: {stop}"))?;
        if ids.len() <= MAX_NODE_POINTS || key.depth >= MAX_DEPTH {
            out.push((key, ids));
            continue;
        }
        let side = 2.0 * halfsize / f64::from(1u32 << key.depth);
        let min = [
            lo[0] + f64::from(key.x) * side,
            lo[1] + f64::from(key.y) * side,
            lo[2] + f64::from(key.z) * side,
        ];
        let cell = side / f64::from(GRID);
        let mut taken = vec![0u64; (GRID * GRID * GRID / 64) as usize];
        let mut mine = Vec::new();
        let mut rest: [Vec<u32>; 8] = Default::default();
        for i in ids {
            let p = &points[i as usize];
            let at = [p.x, p.y, p.z];
            let c = [0, 1, 2].map(|k| (((at[k] - min[k]) / cell) as u32).min(GRID - 1));
            let bit = (c[0] * GRID + c[1]) * GRID + c[2];
            let (word, mask) = ((bit / 64) as usize, 1u64 << (bit % 64));
            if taken[word] & mask == 0 {
                taken[word] |= mask;
                mine.push(i);
            } else {
                let o = (c[0] >= GRID / 2) as u8
                    | (((c[1] >= GRID / 2) as u8) << 1)
                    | (((c[2] >= GRID / 2) as u8) << 2);
                rest[o as usize].push(i);
            }
        }
        out.push((key, mine));
        for (o, ids) in rest.into_iter().enumerate() {
            if !ids.is_empty() {
                stack.push((key.child(o as u8), ids));
            }
        }
    }
    out.sort_by_key(|(k, _)| *k);
    Ok(out)
}

fn put_vlr_header(out: &mut Vec<u8>, user: &str, record: u16, len: usize, description: &str) {
    out.extend_from_slice(&[0, 0]);
    let mut uid = [0u8; 16];
    uid[..user.len()].copy_from_slice(user.as_bytes());
    out.extend_from_slice(&uid);
    out.extend_from_slice(&record.to_le_bytes());
    out.extend_from_slice(&(len as u16).to_le_bytes());
    let mut d = [0u8; 32];
    d[..description.len()].copy_from_slice(description.as_bytes());
    out.extend_from_slice(&d);
}

/// Writes `points` to a COPC file at `scale` (m; 0.001: 1 mm): LAS 1.4,
/// point format 6, LAZ chunks per octree node, one hierarchy page.
/// `points` are reordered (by node).
///
/// Cost: building the octree (~0.1 µs per point per level) and LAZ
/// compression (~1 µs per point); memory: the points (32 bytes each).
/// Exit clause: `budget`, checked per node; the file is then removed.
pub fn write_copc(
    path: &Path,
    points: &[LidarPoint],
    scale: f64,
    budget: &Budget,
) -> Result<CopcWritten, String> {
    let result = write_copc_inner(path, points, scale, budget);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn write_copc_inner(
    path: &Path,
    points: &[LidarPoint],
    scale: f64,
    budget: &Budget,
) -> Result<CopcWritten, String> {
    let run = budget.start();
    let e = |m: String| format!("{}: {m}", path.display());
    if scale.is_nan() || scale <= 0.0 {
        return Err(e("scale must be positive".into()));
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in points {
        for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[k] = lo[k].min(v);
            hi[k] = hi[k].max(v);
        }
    }
    if points.is_empty() {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    let center = [0, 1, 2].map(|k| 0.5 * (lo[k] + hi[k]));
    let halfsize = (0..3)
        .map(|k| 0.5 * (hi[k] - lo[k]))
        .fold(0.0, f64::max)
        .max(scale)
        * (1.0 + 1e-9)
        + scale;
    let offset = center.map(|c| (c / scale).round() * scale);
    let nodes = split(points, center, halfsize, &run)?;
    let laz_vlr = laz::LazVlrBuilder::default()
        .with_point_format(6, 0)
        .map_err(|x| e(x.to_string()))?
        .with_variable_chunk_size()
        .build();
    let mut laz_bytes = Vec::new();
    laz_vlr
        .write_to(&mut laz_bytes)
        .map_err(|x| e(x.to_string()))?;
    let point_data = HEADER_SIZE + VLR_HEADER + INFO_SIZE + VLR_HEADER + laz_bytes.len();
    let file = File::create(path).map_err(|x| e(x.to_string()))?;
    let mut w = BufWriter::new(file);
    let io = |x: std::io::Error| e(x.to_string());
    // Header and VLRs are written last; their room first.
    w.write_all(&vec![0u8; point_data]).map_err(io)?;
    // The LAZ chunk table offset, then a chunk per node.
    w.write_all(&(-1i64).to_le_bytes()).map_err(io)?;
    let mut table = laz::laszip::ChunkTable::default();
    let mut entries = Vec::with_capacity(nodes.len());
    let mut by_return = [0u64; 15];
    for (key, ids) in &nodes {
        run.check().map_err(|stop| e(format!("stopped: {stop}")))?;
        if ids.is_empty() {
            continue;
        }
        let start = w.stream_position().map_err(io)?;
        {
            let mut c = LayeredPointRecordCompressor::new(&mut w);
            c.set_fields_from(laz_vlr.items())
                .map_err(|x| e(x.to_string()))?;
            for &i in ids {
                let p = &points[i as usize];
                c.compress_next(&record(p, scale, offset).map_err(e)?)
                    .map_err(io)?;
                by_return[(p.return_number.clamp(1, 15) - 1) as usize] += 1;
            }
            c.done().map_err(io)?;
        }
        let end = w.stream_position().map_err(io)?;
        table.push(laz::laszip::ChunkTableEntry {
            point_count: ids.len() as u64,
            byte_count: end - start,
        });
        entries.push((*key, start, end - start, ids.len()));
    }
    let table_at = w.stream_position().map_err(io)?;
    table.write_to(&mut w, &laz_vlr).map_err(io)?;
    // The hierarchy: one page, in an extended VLR.
    let evlr_at = w.stream_position().map_err(io)?;
    let page_size = ENTRY_SIZE * entries.len();
    let mut ev = Vec::with_capacity(EVLR_HEADER + page_size);
    ev.extend_from_slice(&[0, 0]);
    let mut uid = [0u8; 16];
    uid[..4].copy_from_slice(b"copc");
    ev.extend_from_slice(&uid);
    ev.extend_from_slice(&1000u16.to_le_bytes());
    ev.extend_from_slice(&(page_size as u64).to_le_bytes());
    let mut d = [0u8; 32];
    d[..15].copy_from_slice(b"EPT hierarchy\0\0");
    ev.extend_from_slice(&d);
    for (key, at, size, n) in &entries {
        for v in [key.depth, key.x, key.y, key.z] {
            ev.extend_from_slice(&v.to_le_bytes());
        }
        ev.extend_from_slice(&at.to_le_bytes());
        ev.extend_from_slice(&(*size as i32).to_le_bytes());
        ev.extend_from_slice(&(*n as i32).to_le_bytes());
    }
    w.write_all(&ev).map_err(io)?;
    // Header, COPC info, LAZ VLR.
    let total = points.len() as u64;
    let mut h = Vec::with_capacity(point_data);
    h.extend_from_slice(b"LASF");
    h.extend_from_slice(&0u16.to_le_bytes()); // file source
    h.extend_from_slice(&16u16.to_le_bytes()); // global encoding: WKT
    h.extend_from_slice(&[0u8; 16]); // project GUID
    h.extend_from_slice(&[1, 4]);
    let mut sys = [0u8; 32];
    sys[..7].copy_from_slice(b"OpenOHL");
    h.extend_from_slice(&sys);
    let software = format!("OpenOHL {}", env!("CARGO_PKG_VERSION"));
    let mut sw = [0u8; 32];
    let n = software.len().min(31);
    sw[..n].copy_from_slice(&software.as_bytes()[..n]);
    h.extend_from_slice(&sw);
    h.extend_from_slice(&1u16.to_le_bytes()); // day
    h.extend_from_slice(&2026u16.to_le_bytes()); // year
    h.extend_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    h.extend_from_slice(&(point_data as u32).to_le_bytes());
    h.extend_from_slice(&2u32.to_le_bytes()); // VLRs
    h.push(6 | 0x80); // format 6, compressed
    h.extend_from_slice(&30u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 4 + 20]); // legacy counts
    for _ in 0..3 {
        h.extend_from_slice(&scale.to_le_bytes());
    }
    for o in offset {
        h.extend_from_slice(&o.to_le_bytes());
    }
    for k in 0..3 {
        h.extend_from_slice(&hi[k].to_le_bytes());
        h.extend_from_slice(&lo[k].to_le_bytes());
    }
    h.extend_from_slice(&0u64.to_le_bytes()); // waveform
    h.extend_from_slice(&evlr_at.to_le_bytes());
    h.extend_from_slice(&1u32.to_le_bytes()); // EVLRs
    h.extend_from_slice(&total.to_le_bytes());
    for n in by_return {
        h.extend_from_slice(&n.to_le_bytes());
    }
    debug_assert_eq!(h.len(), HEADER_SIZE);
    put_vlr_header(&mut h, "copc", 1, INFO_SIZE, "COPC info");
    for v in center {
        h.extend_from_slice(&v.to_le_bytes());
    }
    h.extend_from_slice(&halfsize.to_le_bytes());
    h.extend_from_slice(&(2.0 * halfsize / f64::from(GRID)).to_le_bytes());
    h.extend_from_slice(&(evlr_at + EVLR_HEADER as u64).to_le_bytes());
    h.extend_from_slice(&(page_size as u64).to_le_bytes());
    h.extend_from_slice(&[0u8; 16 + 88]); // GPS times, reserved
    put_vlr_header(
        &mut h,
        "laszip encoded",
        22204,
        laz_bytes.len(),
        "laz variable chunks",
    );
    h.extend_from_slice(&laz_bytes);
    debug_assert_eq!(h.len(), point_data);
    w.seek(SeekFrom::Start(0)).map_err(io)?;
    w.write_all(&h).map_err(io)?;
    w.write_all(&(table_at as i64).to_le_bytes()).map_err(io)?;
    w.flush().map_err(io)?;
    Ok(CopcWritten {
        points: total,
        nodes: entries.len(),
        depth: entries.iter().map(|e| e.0.depth).max().unwrap_or(0),
    })
}
