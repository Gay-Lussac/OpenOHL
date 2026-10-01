# OLS CAD Roadmap

The roadmap is organised in **phases**. Each phase ends with a usable, tested deliverable. Phases map 1:1 to GitHub **milestones**, and each bullet becomes one or more **issues** labelled by module (`module:*`).

Guiding principle: **headless engine first, GUI second.** Every calculation must be usable from Python/CLI and covered by validation cases before it gets a user interface.

---

## Phase 0 — Foundations
*Goal: a project people can contribute to.*

- [ ] Choose license (see ADR-0001)
- [ ] Choose tech stack: language, GUI toolkit, 3D engine (ADR-0002)
- [ ] Define the plugin / extension mechanism (ADR-0003)
- [ ] Define the open project file format (ADR-0004)
- [ ] Monorepo layout, packaging, CI (lint, type-check, tests), pre-commit
- [ ] Units system (SI internally, US customary + SI at the I/O boundary)
- [ ] Documentation site skeleton (theory manual + API reference)
- [ ] Validation/benchmark framework (case format + reference results + tolerance reporting)
- [ ] Contribution guide, code of conduct, clean-room policy

**Deliverable:** empty-but-wired repo, CI green, ADRs accepted.

## Phase 1 — Calculation core (headless)
*Goal: trustworthy single-section sag-tension engine.*

- [ ] `ols-cables`: conductor/cable library — area, diameter, weight, RTS, modulus, thermal coefficients, bimetallic (core/outer) properties
- [ ] Stress-strain models: linear elastic, polynomial (initial / final / creep curves, per CIGRE/Aluminum Association practice), bimetallic split
- [ ] Creep models: equivalent temperature, time-based (IEC/CIGRE TB 324 approaches)
- [ ] `ols-sagtension`: exact catenary, parabola approximation, inclined spans, sag/tension/length/slack
- [ ] Change-of-state equation; initial / final-after-creep / final-after-load conditions
- [ ] Ruling-span (equivalent span) method for tension sections
- [ ] Weather cases (temperature, wind, ice, combined) and tension limit criteria (% RTS, catenary constant, H/w)
- [ ] Stringing charts output (CSV / PDF)
- [ ] CLI: `ols sagtension project.json`
- [ ] Validation: CIGRE TB 324 examples, textbook cases, hand calcs

**Deliverable:** `pip install ols-cad`, reproducible sag-tension tables validated against published references.

## Phase 2 — Line model & terrain
*Goal: a real line described in data, with a ground profile.*

- [ ] `ols-core`: project data model — alignment, PIs, stations, structures, attachment sets, wire sections, spans
- [ ] Alignment: PIs, line angles, stationing, offsets, coordinate systems (EPSG via PROJ)
- [ ] `ols-terrain`: survey point import (CSV/XYZ), feature codes, breaklines, TIN generation, profile extraction (centreline + side profiles)
- [ ] Structure library v1: simple framing with attachment points (x, y, z), insulator definitions (suspension, strain, post)
- [ ] Multi-span sections: wire placed between attachment points over terrain
- [ ] Suspension insulator swing (rigid-body) for level/inclined spans
- [ ] `ols-clearance` v1: ground clearance, feature clearance, clearance envelopes per weather case
- [ ] Plan & profile data output (SVG/DXF) — no GUI yet

**Deliverable:** load a survey + structure positions, compute wire positions and ground clearance violations from CLI.

## Phase 3 — Desktop application MVP
*Goal: engineers can build and inspect a line interactively.*

- [ ] Application shell, document model, undo/redo, command system
- [ ] Plan view, profile view, 3D view (synchronised selection)
- [ ] Interactive structure spotting (add/move/delete structures on profile)
- [ ] Wire stringing UI (sections, tension criteria, weather cases)
- [ ] Clearance violation display and reports
- [ ] Library editors (cables, insulators, structures, weather cases)
- [ ] Plugin manager UI
- [ ] Python scripting console inside the application

**Deliverable:** alpha release of the desktop app.

## Phase 4 — Loading & structural checks
*Goal: design-code compliant loading.*

- [ ] `ols-loads`: load case framework (weather × load factors × wire condition)
- [ ] Design code plugins: NESC (ANSI C2), CSA C22.3, IEC 60826, EN 50341 (+ national NNAs), ASCE MOP 74, GO 95 — each as a separate plugin
- [ ] Structure capacity Method 1: allowable spans (wind/weight span, line angle)
- [ ] Method 2: interaction diagrams / allowable load trees
- [ ] Finite-element wire system: full tension section with non-linear cables, insulator strings, unbalanced ice, broken wire, longitudinal loads
- [ ] Structure loads output (attachment point loads per load case)
- [ ] Usage / utilisation reports

**Deliverable:** code-checked line design with structure loading reports.

## Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

- [ ] `ols-structures-fe`: 3D frame/truss FE analysis of lattice towers and poles (geometric non-linearity, guys) — standalone module
- [ ] Wood/steel/concrete pole libraries and checks
- [ ] LiDAR: LAS/LAZ import, classification, vegetation & encroachment analysis, as-built wire extraction/fitting
- [ ] Thermal rating: IEEE 738, CIGRE TB 601 / TB 207, max operating temperature from clearance
- [ ] Optimum (automatic) structure spotting — cost-based optimisation
- [ ] Galloping ellipses, blowout, phase-to-phase / circuit-to-circuit clearances
- [ ] Drafting: plan & profile sheets, title blocks, DXF/DWG-compatible output, PDF
- [ ] Bill of materials & cost estimation
- [ ] Interop: LandXML, IFC, Shapefile/GeoJSON/GeoPackage, KML, CSV; import of common neutral formats from other tools

**Deliverable:** beta release usable on real projects (with independent verification).

## Phase 6 — Ecosystem & 1.0
- [ ] Stable plugin SDK (semantic versioning, compatibility guarantees)
- [ ] Plugin registry / index
- [ ] Complete theory manual and validation report
- [ ] Translations (i18n)
- [ ] Public release, governance model, maintainers team

---

## Cross-cutting (every phase)
- Validation cases for every calculation, with references
- Theory documentation written alongside the code
- Performance benchmarks (large lines: 500+ structures, LiDAR with 10⁸ points)
- Backwards compatibility of the project file format (versioned schema + migrations)
