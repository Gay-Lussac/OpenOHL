// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The spatial chunk grid (DESIGN_GOALS §5, ADR-0006, ADR-0008).
//!
//! The plan of a project is divided into a fixed **quadtree of square
//! tiles** in the project's coordinate system, independent of any
//! alignment. Level 0 tiles are [`Grid::size`] wide (256 m by default);
//! level `L` tiles are `size · 2^L` wide — coarser for `L > 0`, finer for
//! `L < 0`. Every tile has one parent (`L + 1`) and four children
//! (`L − 1`), and tiles of all levels share the grid origin, so they nest
//! exactly.
//!
//! Objects are placed **loosely** ([`Grid::home`]): each object has one
//! home tile, at the finest level whose tiles are at least as wide as the
//! object, containing the centre of its bounds. Its bounds then lie within
//! the home tile grown by half a tile on every side ([`Grid::loose_bounds`]),
//! so a query looks at a known range of tiles per level ([`Grid::query`])
//! and a long object (a section of several kilometres) is not split or
//! pushed up to the top of the tree by crossing a tile border.

use std::fmt;

/// Plan bounds (x, y), metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Lowest x and y.
    pub min: [f64; 2],
    /// Highest x and y.
    pub max: [f64; 2],
}

impl Bounds {
    /// The bounds of two corners (in any order).
    pub fn new(a: [f64; 2], b: [f64; 2]) -> Self {
        Self {
            min: [a[0].min(b[0]), a[1].min(b[1])],
            max: [a[0].max(b[0]), a[1].max(b[1])],
        }
    }

    /// The bounds of points (x, y, …); `None` without finite points.
    pub fn of_points<'a>(points: impl IntoIterator<Item = &'a [f64; 3]>) -> Option<Self> {
        let mut b: Option<Self> = None;
        for p in points {
            if !(p[0].is_finite() && p[1].is_finite()) {
                continue;
            }
            let q = [p[0], p[1]];
            b = Some(match b {
                None => Self::new(q, q),
                Some(b) => b.union(&Self::new(q, q)),
            });
        }
        b
    }

    /// Smallest bounds holding both.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: [self.min[0].min(other.min[0]), self.min[1].min(other.min[1])],
            max: [self.max[0].max(other.max[0]), self.max[1].max(other.max[1])],
        }
    }

    /// Whether they overlap (touching counts).
    pub fn intersects(&self, other: &Self) -> bool {
        self.min[0] <= other.max[0]
            && other.min[0] <= self.max[0]
            && self.min[1] <= other.max[1]
            && other.min[1] <= self.max[1]
    }

    /// Whether `other` lies inside (touching counts).
    pub fn contains(&self, other: &Self) -> bool {
        self.min[0] <= other.min[0]
            && self.min[1] <= other.min[1]
            && other.max[0] <= self.max[0]
            && other.max[1] <= self.max[1]
    }

    /// Grown by `margin` on every side.
    pub fn grown(&self, margin: f64) -> Self {
        Self {
            min: [self.min[0] - margin, self.min[1] - margin],
            max: [self.max[0] + margin, self.max[1] + margin],
        }
    }

    /// Larger of width and height.
    pub fn extent(&self) -> f64 {
        (self.max[0] - self.min[0]).max(self.max[1] - self.min[1])
    }

    /// Centre.
    pub fn centre(&self) -> [f64; 2] {
        [
            0.5 * (self.min[0] + self.max[0]),
            0.5 * (self.min[1] + self.max[1]),
        ]
    }
}

/// A tile of the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    /// Level: 0 = base tiles, positive coarser, negative finer.
    pub level: i8,
    /// Column (x), from the grid origin.
    pub x: i64,
    /// Row (y), from the grid origin.
    pub y: i64,
}

impl Tile {
    /// The tile of the next coarser level holding this one.
    pub fn parent(&self) -> Self {
        Self {
            level: self.level + 1,
            x: self.x.div_euclid(2),
            y: self.y.div_euclid(2),
        }
    }

    /// The four tiles of the next finer level inside this one.
    pub fn children(&self) -> [Self; 4] {
        let (level, x, y) = (self.level - 1, 2 * self.x, 2 * self.y);
        [
            Self { level, x, y },
            Self { level, x: x + 1, y },
            Self { level, x, y: y + 1 },
            Self {
                level,
                x: x + 1,
                y: y + 1,
            },
        ]
    }

    /// Parses a key `level/x/y` ([`fmt::Display`]).
    pub fn parse(key: &str) -> Result<Self, String> {
        let parts: Vec<&str> = key.trim().split('/').collect();
        let bad = || format!("\"{key}\" is not a tile: use LEVEL/X/Y, e.g. 0/10156/4687");
        let [l, x, y] = parts[..] else {
            return Err(bad());
        };
        Ok(Self {
            level: l.parse().map_err(|_| bad())?,
            x: x.parse().map_err(|_| bad())?,
            y: y.parse().map_err(|_| bad())?,
        })
    }
}

impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.level, self.x, self.y)
    }
}

/// A rectangle of tiles of one level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRange {
    /// Level.
    pub level: i8,
    /// First and last column.
    pub x: [i64; 2],
    /// First and last row.
    pub y: [i64; 2],
}

impl TileRange {
    /// Number of tiles.
    pub fn len(&self) -> u64 {
        ((self.x[1] - self.x[0] + 1) as u64) * ((self.y[1] - self.y[0] + 1) as u64)
    }

    /// Never empty (a range holds at least one tile).
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Whether `tile` is in the range.
    pub fn contains(&self, tile: &Tile) -> bool {
        tile.level == self.level
            && (self.x[0]..=self.x[1]).contains(&tile.x)
            && (self.y[0]..=self.y[1]).contains(&tile.y)
    }

    /// The tiles, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = Tile> + '_ {
        (self.y[0]..=self.y[1]).flat_map(move |y| {
            (self.x[0]..=self.x[1]).map(move |x| Tile {
                level: self.level,
                x,
                y,
            })
        })
    }
}

/// The grid of a project.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Width of level-0 tiles, m.
    pub size: f64,
    /// Corner shared by all tiles, m (x, y).
    pub origin: [f64; 2],
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            size: Self::DEFAULT_SIZE,
            origin: [0.0, 0.0],
        }
    }
}

impl Grid {
    /// Default width of level-0 tiles, m (ADR-0006).
    pub const DEFAULT_SIZE: f64 = 256.0;
    /// Finest level (1 m tiles with the default size).
    pub const MIN_LEVEL: i8 = -8;
    /// Coarsest level (about 4 300 km tiles with the default size).
    pub const MAX_LEVEL: i8 = 14;

    /// A grid with level-0 tiles `size` wide, origin (0, 0).
    pub fn new(size: f64) -> Result<Self, String> {
        if !(size.is_finite() && size > 0.0) {
            return Err(format!("grid tile size {size} m: must be positive"));
        }
        Ok(Self {
            size,
            origin: [0.0, 0.0],
        })
    }

    /// Width of tiles of `level`, m.
    pub fn tile_size(&self, level: i8) -> f64 {
        self.size * 2f64.powi(level as i32)
    }

    /// The tile of `level` containing (x, y) (a point on a border belongs
    /// to the tile above and to the right).
    pub fn tile_at(&self, level: i8, p: [f64; 2]) -> Tile {
        let s = self.tile_size(level);
        Tile {
            level,
            x: ((p[0] - self.origin[0]) / s).floor() as i64,
            y: ((p[1] - self.origin[1]) / s).floor() as i64,
        }
    }

    /// The area of a tile.
    pub fn bounds(&self, tile: &Tile) -> Bounds {
        let s = self.tile_size(tile.level);
        let x = self.origin[0] + tile.x as f64 * s;
        let y = self.origin[1] + tile.y as f64 * s;
        Bounds::new([x, y], [x + s, y + s])
    }

    /// The tiles of `level` that `area` touches.
    pub fn covering(&self, area: &Bounds, level: i8) -> TileRange {
        let (a, b) = (self.tile_at(level, area.min), self.tile_at(level, area.max));
        TileRange {
            level,
            x: [a.x, b.x],
            y: [a.y, b.y],
        }
    }

    /// The finest level (at least `min_level`) whose tiles are as wide as
    /// `extent`, m.
    pub fn level_for(&self, extent: f64, min_level: i8) -> i8 {
        let mut level = min_level.clamp(Self::MIN_LEVEL, Self::MAX_LEVEL);
        while level < Self::MAX_LEVEL && self.tile_size(level) < extent {
            level += 1;
        }
        level
    }

    /// The home tile of an object with these bounds: at the finest level
    /// (at least 0) whose tiles are as wide as the object, holding its
    /// centre.
    pub fn home(&self, bounds: &Bounds) -> Tile {
        let level = self.level_for(bounds.extent(), 0);
        self.tile_at(level, bounds.centre())
    }

    /// Where the objects at home in `tile` can reach: the tile grown by
    /// half its width on every side.
    pub fn loose_bounds(&self, tile: &Tile) -> Bounds {
        self.bounds(tile).grown(0.5 * self.tile_size(tile.level))
    }

    /// The home tiles of `level` whose objects may reach into `area`.
    pub fn query(&self, area: &Bounds, level: i8) -> TileRange {
        self.covering(&area.grown(0.5 * self.tile_size(level)), level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_nest_across_levels() {
        let g = Grid::default();
        let p = [2_600_123.4, 1_199_999.9];
        let t = g.tile_at(0, p);
        assert_eq!((t.x, t.y), (10156, 4687));
        assert!(g.bounds(&t).contains(&Bounds::new(p, p)));
        // Parent and children agree with tile_at on every level.
        for level in -3..6 {
            let t = g.tile_at(level, p);
            assert_eq!(t.parent(), g.tile_at(level + 1, p));
            assert!(t.parent().children().contains(&t));
            let cb = t
                .children()
                .iter()
                .map(|c| g.bounds(c))
                .reduce(|a, b| a.union(&b));
            assert_eq!(cb.unwrap(), g.bounds(&t));
        }
        // Negative coordinates: floor, not truncation.
        let n = g.tile_at(0, [-1.0, -256.0]);
        assert_eq!((n.x, n.y), (-1, -1));
        assert_eq!(
            n.parent(),
            Tile {
                level: 1,
                x: -1,
                y: -1
            }
        );
        assert_eq!(Tile::parse(&t.to_string()).unwrap(), t);
        assert!(Tile::parse("0/1").is_err());
    }

    #[test]
    fn loose_home_tiles_hold_their_objects() {
        let g = Grid::default();
        // A pylon: level 0. A 700 m span: level 2 (1024 m). A 9 km section
        // across many borders: level 6 (16 384 m), not the top of the tree.
        let cases = [
            (Bounds::new([100.0, 100.0], [110.0, 104.0]), 0),
            (Bounds::new([200.0, 10.0], [900.0, 60.0]), 2),
            (Bounds::new([-4_500.0, -20.0], [4_500.0, 20.0]), 6),
        ];
        for (b, level) in cases {
            let home = g.home(&b);
            assert_eq!(home.level, level, "{b:?}");
            assert!(g.loose_bounds(&home).contains(&b), "{b:?}");
            // Any area touching the object finds its home tile.
            for corner in [b.min, b.max, [b.min[0], b.max[1]]] {
                let area = Bounds::new(corner, corner).grown(0.1);
                assert!(g.query(&area, home.level).contains(&home));
            }
        }
        // A query is a small range of tiles per level.
        let area = Bounds::new([0.0, 0.0], [300.0, 300.0]);
        assert_eq!(g.query(&area, 0).len(), 9);
        assert_eq!(g.covering(&area, 0).len(), 4);
    }

    #[test]
    fn level_for_sizes() {
        let g = Grid::new(100.0).unwrap();
        assert_eq!(g.level_for(0.0, 0), 0);
        assert_eq!(g.level_for(100.0, 0), 0);
        assert_eq!(g.level_for(100.1, 0), 1);
        assert_eq!(g.level_for(30.0, -3), -1);
        assert_eq!(g.level_for(25.0, -3), -2);
        assert_eq!(g.level_for(1e12, 0), Grid::MAX_LEVEL);
        assert!(Grid::new(0.0).is_err());
    }
}
