//! Paper sizes and the optimised sheet layout.
//!
//! The standard sizes a designer may tick ([`PAPER_SIZES`]), and the
//! choice of paper and orientation that lays a route out on sheets with
//! the fewest problems and the least empty paper ([`candidates`]): every
//! allowed size in each allowed orientation is laid out as
//! [`crate::sheet_layout`] would, at the options' scales (never changed),
//! and the candidates are ranked by
//!
//! 1. the number of warnings (a span longer than the sheet, a profile
//!    whose datum must step or whose structure tops are cut, elevations
//!    overlapping or leaving the plan, no free break within reach),
//! 2. the **empty paper** in the panels (owner, 2026-10-28: sheets
//!    should mostly avoid empty space): the panels' width past the route
//!    drawn and the profile panel's height above the profile, summed over
//!    the sheets (the furniture the template places is not counted),
//! 3. the paper used (sum of the sheets' areas),
//! 4. the number of sheets,
//! 5. the largest sheet.
//!
//! With different sizes allowed ([`Optimise::mixed`]) a candidate may
//! put each sheet on another size **of the same standard** (ISO A,
//! ISO B, ANSI, ARCH) in the same orientation, and, with both
//! orientations ticked, one more candidate per standard may also turn
//! each sheet (landscape where the profile is low, portrait over a
//! valley): the breaks and the pages are chosen together (dynamic
//! programming over the places a sheet may end, with the profile's fit
//! estimated per sheet from heights joined stretch by stretch, the same
//! order: warnings, empty paper, paper, sheets), so a short last sheet
//! takes a smaller size; the sheets chosen are laid out in full and
//! ranked with the others.
//!
//! Sizes: ISO 216 (A and B series); the elongated A sizes (A3×3 …) as
//! listed for ISO 5457 / IS 10711; ANSI/ASME Y14.1 (A–E) and ARCH (A–E,
//! E1) from their inch sizes (exact, 25.4 mm per inch).

use crate::layout::{
    Stop, StretchHeights, can_break, ends, layout_planned, profile_mm, route_stops,
};
use crate::{Options, Page, RouteDrawing, Sheet, SheetLayout};

/// A family of paper sizes: sizes of one standard may be mixed in one
/// drawing set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Standard {
    /// ISO 216 A series, with the elongated A sizes.
    IsoA,
    /// ISO 216 B series.
    IsoB,
    /// ANSI/ASME Y14.1 (US engineering).
    Ansi,
    /// ARCH (US architectural).
    Arch,
}

impl Standard {
    /// Name for the user.
    pub fn name(self) -> &'static str {
        match self {
            Self::IsoA => "ISO A",
            Self::IsoB => "ISO B",
            Self::Ansi => "ANSI",
            Self::Arch => "ARCH",
        }
    }
}

/// A standard paper size, mm, its longer side first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaperSize {
    /// Name, e.g. `A1`, `A3x3`, `ANSI D`.
    pub name: &'static str,
    /// Its standard.
    pub standard: Standard,
    /// The longer side, mm.
    pub long: f64,
    /// The shorter side, mm.
    pub short: f64,
    /// Ticked by default.
    pub default: bool,
}

impl PaperSize {
    /// The size as a landscape page.
    pub fn page(&self) -> Page {
        Page {
            name: self.name.to_owned(),
            width: self.long,
            height: self.short,
        }
    }
}

const IN: f64 = 25.4;

const fn iso_a(name: &'static str, long: f64, short: f64, default: bool) -> PaperSize {
    PaperSize {
        name,
        standard: Standard::IsoA,
        long,
        short,
        default,
    }
}

const fn size(name: &'static str, standard: Standard, long: f64, short: f64) -> PaperSize {
    PaperSize {
        name,
        standard,
        long,
        short,
        default: false,
    }
}

/// The standard sizes offered, by standard and size.
pub const PAPER_SIZES: &[PaperSize] = &[
    iso_a("A0", 1189.0, 841.0, true),
    iso_a("A1", 841.0, 594.0, true),
    iso_a("A2", 594.0, 420.0, true),
    iso_a("A3", 420.0, 297.0, true),
    iso_a("A4", 297.0, 210.0, true),
    iso_a("A3x3", 891.0, 420.0, false),
    iso_a("A3x4", 1189.0, 420.0, false),
    iso_a("A3x5", 1486.0, 420.0, false),
    iso_a("A3x6", 1783.0, 420.0, false),
    iso_a("A3x7", 2080.0, 420.0, false),
    iso_a("A4x3", 630.0, 297.0, false),
    iso_a("A4x4", 841.0, 297.0, false),
    iso_a("A4x5", 1051.0, 297.0, false),
    iso_a("A4x6", 1261.0, 297.0, false),
    iso_a("A4x7", 1471.0, 297.0, false),
    iso_a("A4x8", 1682.0, 297.0, false),
    iso_a("A4x9", 1892.0, 297.0, false),
    iso_a("A2x3", 1261.0, 594.0, false),
    iso_a("A2x4", 1682.0, 594.0, false),
    iso_a("A2x5", 2102.0, 594.0, false),
    iso_a("A1x3", 1783.0, 841.0, false),
    iso_a("A1x4", 2378.0, 841.0, false),
    iso_a("A0x2", 1682.0, 1189.0, false),
    iso_a("A0x3", 2523.0, 1189.0, false),
    size("B1", Standard::IsoB, 1000.0, 707.0),
    size("B2", Standard::IsoB, 707.0, 500.0),
    size("B3", Standard::IsoB, 500.0, 353.0),
    size("B4", Standard::IsoB, 353.0, 250.0),
    size("ANSI A", Standard::Ansi, 11.0 * IN, 8.5 * IN),
    size("ANSI B", Standard::Ansi, 17.0 * IN, 11.0 * IN),
    size("ANSI C", Standard::Ansi, 22.0 * IN, 17.0 * IN),
    size("ANSI D", Standard::Ansi, 34.0 * IN, 22.0 * IN),
    size("ANSI E", Standard::Ansi, 44.0 * IN, 34.0 * IN),
    size("ARCH A", Standard::Arch, 12.0 * IN, 9.0 * IN),
    size("ARCH B", Standard::Arch, 18.0 * IN, 12.0 * IN),
    size("ARCH C", Standard::Arch, 24.0 * IN, 18.0 * IN),
    size("ARCH D", Standard::Arch, 36.0 * IN, 24.0 * IN),
    size("ARCH E1", Standard::Arch, 42.0 * IN, 30.0 * IN),
    size("ARCH E", Standard::Arch, 48.0 * IN, 36.0 * IN),
];

/// `name` without spaces, `×` as `x`, upper case: how names compare.
fn key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '×' { 'x' } else { c })
        .collect::<String>()
        .to_ascii_uppercase()
}

/// The standard size called `name` (`A3x3`, `a3×3`, `ANSI D`, `ansid`).
pub fn paper_size(name: &str) -> Option<&'static PaperSize> {
    let k = key(name.trim_end_matches(" portrait"));
    PAPER_SIZES.iter().find(|p| key(p.name) == k)
}

/// The sizes ticked by default (ISO A0–A4).
pub fn default_sizes() -> Vec<String> {
    PAPER_SIZES
        .iter()
        .filter(|p| p.default)
        .map(|p| p.name.to_owned())
        .collect()
}

/// What the optimiser may choose from.
#[derive(Debug, Clone, PartialEq)]
pub struct Optimise {
    /// The sizes allowed, as landscape pages (standard or custom).
    pub sizes: Vec<Page>,
    /// Landscape allowed.
    pub landscape: bool,
    /// Portrait allowed.
    pub portrait: bool,
    /// Different sizes of one standard on the sheets of a route.
    pub mixed: bool,
    /// Only the candidates that turn each sheet as suits it (both
    /// orientations allowed): what [`common_choice`] keeps when it chose
    /// one of them.
    pub turned_only: bool,
}

impl Optimise {
    /// The sizes called `names` ([`crate::page_format`]: standard or
    /// `WIDTHxHEIGHT`), both orientations.
    pub fn from_names(names: &[String], mixed: bool) -> Result<Self, String> {
        let sizes = names
            .iter()
            .map(|n| crate::page_format(n).ok_or_else(|| format!("unknown paper size \"{n}\"")))
            .collect::<Result<Vec<_>, _>>()?;
        if sizes.is_empty() {
            return Err("tick at least one paper size to optimise the layout".into());
        }
        Ok(Self {
            sizes,
            landscape: true,
            portrait: true,
            mixed,
            turned_only: false,
        })
    }

    fn orientations(&self) -> Vec<bool> {
        let mut v = Vec::new();
        if self.landscape || !self.portrait {
            v.push(false);
        }
        if self.portrait {
            v.push(true);
        }
        v
    }
}

/// How a candidate ranks: warnings, empty paper (per 10 cm²), paper (cm²),
/// sheets, the largest sheet (mm²).
type Rank = (usize, i64, i64, usize, i64);

/// One way of laying the route out.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// What it is: `A2 landscape`, or `ISO A portrait, mixed`.
    pub label: String,
    /// The key that picks the same candidate on another route.
    pub key: String,
    /// Each sheet's page.
    pub pages: Vec<Page>,
    /// Warnings over its sheets.
    pub warnings: usize,
    /// Paper used, m².
    pub area: f64,
    /// Route drawn over the route the sheets could hold (0–1).
    pub fill: f64,
    /// Empty paper in the sheets' panels, m² (see the module).
    pub empty: f64,
    /// Its sheets.
    pub layout: Vec<SheetLayout>,
}

impl Candidate {
    /// Sheets.
    pub fn sheets(&self) -> usize {
        self.pages.len()
    }

    /// The sizes used, `A2 ×3, A3 ×1`.
    pub fn sizes(&self) -> String {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for p in &self.pages {
            match counts.iter_mut().find(|(n, _)| *n == p.name) {
                Some(c) => c.1 += 1,
                None => counts.push((p.name.clone(), 1)),
            }
        }
        counts
            .iter()
            .map(|(n, c)| format!("{n} ×{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn rank(&self) -> Rank {
        let largest = self
            .pages
            .iter()
            .map(|p| p.width * p.height)
            .fold(0.0, f64::max);
        (
            self.warnings,
            (self.empty * 1e3).round() as i64,
            (self.area * 1e4).round() as i64,
            self.pages.len(),
            largest.round() as i64,
        )
    }
}

fn oriented(p: &Page, portrait: bool) -> Page {
    if portrait {
        p.clone().portrait()
    } else {
        Page {
            width: p.width.max(p.height),
            height: p.width.min(p.height),
            ..p.clone()
        }
    }
}

fn orientation_name(portrait: bool) -> &'static str {
    if portrait { "portrait" } else { "landscape" }
}

/// The sizes of `pages` grouped by standard (at least two pages each;
/// custom sizes left out).
fn by_standard(pages: &[Page]) -> Vec<(Standard, Vec<Page>)> {
    let mut standards: Vec<Standard> = pages.iter().filter_map(standard_of).collect();
    standards.sort();
    standards.dedup();
    standards
        .into_iter()
        .map(|s| {
            let group: Vec<Page> = pages
                .iter()
                .filter(|p| standard_of(p) == Some(s))
                .cloned()
                .collect();
            (s, group)
        })
        .filter(|(_, g)| g.len() >= 2)
        .collect()
}

/// The standard of `p` (none: a custom size).
fn standard_of(p: &Page) -> Option<Standard> {
    paper_size(&p.name).map(|s| s.standard)
}

/// Every candidate for route `d` with options `o`, best first (see the
/// module). Expected cost: one layout per size and orientation, and with
/// `mixed` one sheet layout per place a sheet may start × size × place
/// it may end (typically well under a second per route; the profile fit
/// of a sheet is the cost).
pub fn candidates(d: &RouteDrawing, o: &Options, opt: &Optimise) -> Vec<Candidate> {
    let d = &d.shown(o.plan_half_width);
    let stations = route_stops(d, o);
    let mut out = Vec::new();
    for portrait in opt.orientations() {
        if opt.turned_only && opt.landscape && opt.portrait {
            break;
        }
        let pages: Vec<Page> = opt.sizes.iter().map(|p| oriented(p, portrait)).collect();
        for (k, page) in pages.iter().enumerate() {
            let layout = crate::layout::sheet_layout_on(d, o, &stations, page);
            out.push(candidate(
                format!("{} {}", opt.sizes[k].name, orientation_name(portrait)),
                format!("{}|{}", opt.sizes[k].name, orientation_name(portrait)),
                layout,
            ));
        }
        if opt.mixed {
            for (s, group) in by_standard(&pages) {
                let plan = mixed_plan(d, o, &stations, &group);
                let layout = layout_planned(d, o, &stations, &plan);
                out.push(candidate(
                    format!("{} {}, sizes mixed", s.name(), orientation_name(portrait)),
                    format!("{}|{}|mixed", s.name(), orientation_name(portrait)),
                    layout,
                ));
            }
        }
    }
    // Both orientations ticked: each sheet turned as suits it (of one
    // size, or of one standard with sizes mixed).
    if opt.landscape && opt.portrait {
        let turned = |p: &Page| vec![oriented(p, false), oriented(p, true)];
        let groups: Vec<(String, String, Vec<Page>)> = if opt.mixed {
            let pages: Vec<Page> = opt.sizes.iter().flat_map(turned).collect();
            by_standard(&pages)
                .into_iter()
                .map(|(s, g)| {
                    (
                        format!("{} landscape and portrait, sizes mixed", s.name()),
                        format!("{}|both|mixed", s.name()),
                        g,
                    )
                })
                .collect()
        } else {
            opt.sizes
                .iter()
                .map(|p| {
                    (
                        format!("{} landscape and portrait", p.name),
                        format!("{}|both", p.name),
                        turned(p),
                    )
                })
                .collect()
        };
        for (label, key, group) in groups {
            let plan = mixed_plan(d, o, &stations, &group);
            out.push(candidate(
                label,
                key,
                layout_planned(d, o, &stations, &plan),
            ));
        }
    }
    out.sort_by_key(Candidate::rank);
    out
}

fn candidate(label: String, key: String, layout: Vec<SheetLayout>) -> Candidate {
    let pages: Vec<Page> = layout.iter().map(|s| s.page.clone()).collect();
    let drawn: f64 = layout.iter().map(|s| s.to - s.from).sum();
    let held: f64 = layout.iter().map(|s| s.reach.max(s.to - s.from)).sum();
    Candidate {
        label,
        key,
        warnings: layout.iter().map(|s| s.warnings.len()).sum(),
        area: pages.iter().map(|p| p.width * p.height).sum::<f64>() / 1e6,
        fill: if held > 0.0 { drawn / held } else { 0.0 },
        empty: layout.iter().map(|s| s.empty).sum::<f64>() / 1e6,
        pages,
        layout,
    }
}

/// The breaks and pages of `pages` (one standard; one orientation, or
/// both) with the fewest warnings, then the least empty paper, then the
/// least paper, then the fewest sheets.
/// The warnings are estimated per sheet (a sheet running past its reach;
/// the profile's fit from [`StretchHeights`], joined per stretch between
/// stops, so each sheet costs no pass over the drawing); the sheets
/// chosen are then laid out in full and ranked with their own warnings.
fn mixed_plan(
    d: &RouteDrawing,
    o: &Options,
    stations: &[Stop],
    pages: &[Page],
) -> Vec<((usize, usize), Page)> {
    let n = stations.len();
    if n < 2 {
        let page = pages[0].clone();
        return vec![((0, 0), page)];
    }
    let papers: Vec<_> = pages
        .iter()
        .map(|p| crate::layout::Paper::new(d, o, p))
        .collect();
    let heights = StretchHeights::new(d, o, stations);
    // best[i]: the cost of laying out from stop i to the end, and the
    // first sheet (its end, its page).
    type Cost = (usize, f64, f64, usize);
    let mut best: Vec<Option<(Cost, usize, usize)>> = vec![None; n];
    best[n - 1] = Some(((0, 0.0, 0.0, 0), n - 1, 0));
    let key = |a: Cost| {
        (
            a.0,
            (a.1 * 1e-3).round() as i64,
            (a.2 * 1e-2).round() as i64,
            a.3,
        )
    };
    let better = |a: Cost, b: Cost| key(a) < key(b);
    for i in (0..n - 1).rev() {
        if i > 0 && !can_break(stations, i) {
            continue;
        }
        for (k, page) in pages.iter().enumerate() {
            let reach = papers[k].usable_m();
            for j in ends(stations, i, reach) {
                let Some((rest, _, _)) = best[j] else {
                    continue;
                };
                let runs_on = stations[j].station - stations[i].station > reach + 1e-9;
                let warnings =
                    usize::from(runs_on) + heights.warnings(o, i, j, papers[k].available_mm());
                let profile = profile_mm(o, heights.between(i, j));
                let drawn = stations[j].station - stations[i].station;
                let cost = (
                    rest.0 + warnings,
                    rest.1 + papers[k].empty(o, drawn, profile),
                    rest.2 + page.width * page.height,
                    rest.3 + 1,
                );
                if best[i].is_none_or(|(c, _, _)| better(cost, c)) {
                    best[i] = Some((cost, j, k));
                }
            }
        }
    }
    let mut plan = Vec::new();
    let mut i = 0;
    while i < n - 1 {
        let Some((_, j, k)) = best[i] else { break };
        plan.push(((i, j), pages[k].clone()));
        i = j;
    }
    plan
}

/// The best candidate for route `d` ([`candidates`]).
pub fn best(d: &RouteDrawing, o: &Options, opt: &Optimise) -> Option<Candidate> {
    candidates(d, o, opt).into_iter().next()
}

/// Sheet `number` (1-based) of candidate `c` of route `d`, drawn alone.
pub fn draw_one(d: &RouteDrawing, o: &Options, c: &Candidate, number: usize) -> Option<Sheet> {
    crate::layout::sheet_of(d, o, c.layout.clone(), number)
}

/// The sheets of candidate `c` of route `d`, drawn ([`crate::sheets`]).
pub fn draw(d: &RouteDrawing, o: &Options, c: &Candidate) -> Vec<Sheet> {
    crate::layout::draw_sheets(&d.shown(o.plan_half_width), o, c.layout.clone())
}

/// The optimiser narrowed to the one candidate that is best over every
/// route of `routes` together (the least warnings, then empty paper, then paper, then
/// sheets summed), for one paper (or one standard, mixed) on every route.
pub fn common_choice(routes: &[(&RouteDrawing, &Options)], opt: &Optimise) -> Optimise {
    let mut totals: Vec<(String, Rank)> = Vec::new();
    for (d, o) in routes {
        for c in candidates(d, o, opt) {
            let r = c.rank();
            match totals.iter_mut().find(|(k, _)| *k == c.key) {
                Some((_, t)) => {
                    t.0 += r.0;
                    t.1 += r.1;
                    t.2 += r.2;
                    t.3 += r.3;
                    t.4 = t.4.max(r.4);
                }
                None => totals.push((c.key.clone(), r)),
            }
        }
    }
    let Some((key, _)) = totals.into_iter().min_by_key(|(_, r)| *r) else {
        return opt.clone();
    };
    narrowed(opt, &key)
}

/// `opt` narrowed to the candidate called `key`.
fn narrowed(opt: &Optimise, key: &str) -> Optimise {
    let parts: Vec<&str> = key.split('|').collect();
    let both = parts.get(1) == Some(&"both");
    let portrait = parts.get(1) == Some(&"portrait");
    let mixed = parts.get(2) == Some(&"mixed");
    let sizes = if mixed {
        opt.sizes
            .iter()
            .filter(|p| standard_of(p).map(Standard::name) == Some(parts[0]))
            .cloned()
            .collect()
    } else {
        opt.sizes
            .iter()
            .filter(|p| p.name == parts[0])
            .cloned()
            .collect()
    };
    Optimise {
        sizes,
        landscape: both || !portrait,
        portrait: both || portrait,
        mixed,
        turned_only: both,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_found_by_name_and_landscape() {
        assert_eq!(paper_size("a3×3").map(|p| p.long), Some(891.0));
        assert_eq!(paper_size("ANSI D").map(|p| p.short), Some(22.0 * 25.4));
        assert_eq!(paper_size("ansid").map(|p| p.name), Some("ANSI D"));
        assert!(PAPER_SIZES.iter().all(|p| p.long >= p.short));
        let names: Vec<String> = PAPER_SIZES.iter().map(|p| key(p.name)).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
        assert_eq!(default_sizes(), ["A0", "A1", "A2", "A3", "A4"]);
    }

    /// The heights joined stretch by stretch are those of the whole
    /// stretch, exactly (what the mixed search estimates from).
    #[test]
    fn joined_heights_equal_the_whole_stretch() {
        let o = Options {
            vertical_scale: 500.0,
            ..Options::default()
        };
        let d = crate::tests::main_route(&o).shown(o.plan_half_width);
        let stations = route_stops(&d, &o);
        let joined = StretchHeights::new(&d, &o, &stations);
        for i in 0..stations.len() {
            for j in i + 1..stations.len() {
                let whole =
                    crate::layout::heights(&d, &o, stations[i].station, stations[j].station);
                assert_eq!(joined.between(i, j), whole, "{i}–{j}");
            }
        }
    }

    /// The best candidate ranks first; its sheets follow each other along
    /// the route and are drawn on their own pages; a mixed candidate uses
    /// one standard and one orientation.
    #[test]
    fn the_best_layout_is_ranked_first_and_drawn() {
        let o = Options {
            vertical_scale: 500.0,
            ..Options::default()
        };
        let d = crate::tests::main_route(&o);
        let opt = Optimise::from_names(&["A2".into(), "A3".into(), "A4".into(), "B3".into()], true)
            .unwrap();
        let all = candidates(&d, &o, &opt);
        // 4 sizes × 2 orientations, ISO A mixed in each orientation
        // (ISO B has one size: no mixed candidate), and ISO A and ISO B
        // with each sheet turned as suits it.
        assert_eq!(all.len(), 12);
        let best = &all[0];
        assert!(all.iter().all(|c| best.rank() <= c.rank()));
        for w in best.layout.windows(2) {
            assert_eq!(w[0].to, w[1].from);
        }
        for c in all
            .iter()
            .filter(|c| c.key.ends_with("|mixed") && !c.key.contains("|both"))
        {
            let portrait = c.pages[0].height > c.pages[0].width;
            assert!(
                c.pages
                    .iter()
                    .all(|p| standard_of(p) == Some(Standard::IsoA))
            );
            assert!(c.pages.iter().all(|p| (p.height > p.width) == portrait));
            // Never worse than a single size of its group.
            let one = all
                .iter()
                .filter(|x| !x.key.ends_with("|mixed"))
                .filter(|x| standard_of(&x.pages[0]) == Some(Standard::IsoA))
                .filter(|x| (x.pages[0].height > x.pages[0].width) == portrait)
                .map(Candidate::rank)
                .min()
                .unwrap();
            assert!(c.warnings <= one.0, "{} {:?}", c.label, one);
        }
        let sheets = draw(&d, &o, best);
        assert_eq!(sheets.len(), best.sheets());
        for (s, p) in sheets.iter().zip(&best.pages) {
            assert_eq!(&s.page, p);
            assert_eq!((s.drawing.width, s.drawing.height), (p.width, p.height));
        }
        // One sheet alone is that sheet of the whole set.
        let last = draw_one(&d, &o, best, best.sheets()).unwrap();
        assert_eq!(last.svg(), sheets.last().unwrap().svg());
        // The same paper on every route: one size and orientation left
        // (or, mixed, one standard and orientation).
        let fixed = Optimise {
            mixed: false,
            landscape: false,
            ..opt.clone()
        };
        let common = common_choice(&[(&d, &o), (&d, &o)], &fixed);
        assert_eq!(candidates(&d, &o, &common).len(), 1);
        let common = common_choice(&[(&d, &o), (&d, &o)], &opt);
        let n = candidates(&d, &o, &common).len();
        assert!(common.landscape != common.portrait || (common.turned_only && n <= 2));
    }

    /// With both orientations, a candidate turns each sheet as suits it:
    /// never more warnings, nor more empty paper at as many warnings,
    /// than the same sizes in one orientation; empty paper is what the
    /// candidates are ranked by after the warnings.
    #[test]
    fn sheets_turned_as_suits_them_leave_the_least_empty_paper() {
        let o = Options {
            vertical_scale: 500.0,
            ..Options::default()
        };
        let d = crate::tests::main_route(&o);
        let opt = Optimise::from_names(&["A1".into(), "A2".into()], true).unwrap();
        let all = candidates(&d, &o, &opt);
        let turned = all.iter().find(|c| c.key == "ISO A|both|mixed").unwrap();
        for one in all
            .iter()
            .filter(|c| c.key.starts_with("ISO A|") && c.key != turned.key)
        {
            assert!(
                (turned.warnings, turned.empty) <= (one.warnings, one.empty + 1e-3),
                "{} {} {} vs {} {}",
                one.label,
                one.warnings,
                one.empty,
                turned.warnings,
                turned.empty
            );
        }
        for w in all.windows(2) {
            assert!(w[0].warnings < w[1].warnings || w[0].empty <= w[1].empty + 1e-3);
        }
        assert!(all.iter().all(|c| c.empty >= 0.0 && c.empty < c.area));
        // One size: the sheets of it turned.
        let a1 = Optimise::from_names(&["A1".into()], false).unwrap();
        let keys: Vec<String> = candidates(&d, &o, &a1).into_iter().map(|c| c.key).collect();
        assert!(keys.contains(&"A1|both".to_owned()), "{keys:?}");
    }

    #[test]
    fn narrowing_keeps_one_size_or_one_standard() {
        let opt = Optimise::from_names(&["A2".into(), "A3".into(), "B2".into()], true).unwrap();
        let one = narrowed(&opt, "A3|portrait");
        assert_eq!(one.sizes.len(), 1);
        assert!(one.portrait && !one.landscape && !one.mixed);
        let iso = narrowed(&opt, "ISO A|landscape|mixed");
        assert_eq!(
            iso.sizes
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["A2", "A3"]
        );
        assert!(iso.mixed && iso.landscape && !iso.portrait);
        let turned = narrowed(&opt, "ISO A|both|mixed");
        assert!(turned.landscape && turned.portrait && turned.turned_only);
    }
}
