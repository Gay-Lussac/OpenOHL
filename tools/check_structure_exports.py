# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Checks structure exports (IFC4, IFC2x3, SAF) with free tools.

    <ifcopenshell-venv>/bin/python -I tools/check_structure_exports.py FILE_OR_FOLDER...

IFC (``*.ifc``) needs IfcOpenShell (0.9 tried) in the Python running this
script; SAF (``*.xlsx``) needs the standard library only. Prints one block
per file and exits with 1 when any file has an error. Warnings are listed
but do not fail. See docs/development/structure-exports.md, "Checks with
free tools".

IFC, per file:
- ``ifcopenshell.validate.validate(f, logger, express_rules=True)``:
  schema (attribute types and counts, inverse cardinalities, enumerations)
  and the schema's where-rules and global rules. IfcOpenShell's rule runner
  imports pytest only to rewrite ``assert`` statements into readable
  messages; pytest is not installed here, so a small stand-in below gives
  each failed rule its own expression as the message (the rules
  themselves are IfcOpenShell's, unchanged).
- content: units (metre, radian, newton, pascal, kilogram), counts of
  members, profiles, materials and structural analysis entities, GlobalIds
  unique and 22 characters, every IfcMember with a local placement whose
  axes are orthonormal and right-handed, its axis line's ends at its
  analysis member's joints (IFC4 with the analysis model), its body solid
  built by IfcOpenShell's geometry kernel with a volume of about A x L
  (A from Pset_ProfileMechanical, IFC4), property set OHL_Member on every
  member and OHL_Section on every profile with geometry (IFC4).

SAF, per file: the workbook opens (zip, SpreadsheetML), the sheets and
column headers are the exporter's (apps/structure/ohl-structure-export/src/saf.rs,
SAF 2.2.0), names are unique per sheet, the references resolve (members to
cross-sections and joints, cross-sections to materials and composite
shapes, supports to joints, actions to load cases and joints or members,
load cases to load groups), numbers are numbers, and the units are the
metric ones of the headers.
"""

import ast
import math
import os
import re
import sys
import types
import zipfile
import xml.etree.ElementTree as ET


def _pytest_stand_in():
    """A stand-in for ``_pytest.assertion`` (see the module docstring):
    gives every bare ``assert`` its source text as the message."""

    class Messages(ast.NodeTransformer):
        def visit_Assert(self, node):
            if node.msg is None:
                node.msg = ast.Constant(ast.unparse(node.test))
            return node

    def rewrite_asserts(mod, source=None, **_):
        Messages().visit(mod)
        ast.fix_missing_locations(mod)

    rewrite = types.ModuleType("_pytest.assertion.rewrite")
    rewrite.rewrite_asserts = rewrite_asserts
    assertion = types.ModuleType("_pytest.assertion")
    assertion.rewrite = rewrite
    pytest = types.ModuleType("_pytest")
    pytest.assertion = assertion
    sys.modules.setdefault("_pytest", pytest)
    sys.modules.setdefault("_pytest.assertion", assertion)
    sys.modules.setdefault("_pytest.assertion.rewrite", rewrite)


class Report:
    def __init__(self, path):
        self.path = path
        self.errors = []
        self.warnings = []
        self.facts = []

    def error(self, msg):
        self.errors.append(msg)

    def warn(self, msg):
        self.warnings.append(msg)

    def fact(self, msg):
        self.facts.append(msg)

    def print(self):
        state = "FAIL" if self.errors else "ok"
        print(f"== {self.path}: {state} ({len(self.errors)} errors, {len(self.warnings)} warnings)")
        for f in self.facts:
            print(f"   {f}")
        for w in self.warnings:
            print(f"   warning: {w}")
        for e in self.errors:
            print(f"   ERROR: {e}")


# ---------------------------------------------------------------- IFC


def check_ifc(path):
    r = Report(path)
    try:
        import ifcopenshell
        import ifcopenshell.validate
    except ImportError:
        r.error("IfcOpenShell is not importable: run this script with its venv's python")
        return r
    _pytest_stand_in()
    f = ifcopenshell.open(path)
    schema = f.schema
    r.fact(f"IfcOpenShell {ifcopenshell.version}, schema {schema}, {len(list(f))} entities")

    logger = ifcopenshell.validate.json_logger()
    ifcopenshell.validate.validate(f, logger, express_rules=True)
    seen = {}
    for s in logger.statements:
        msg = str(s.get("message")).strip().replace("\n", " ")
        msg = re.sub(r"\s+", " ", msg)
        key = (s.get("level"), s.get("type"), msg[:300])
        seen[key] = seen.get(key, 0) + 1
    for (level, kind, msg), n in sorted(seen.items(), key=str):
        text = f"validate [{kind}] {msg}" + (f" (x{n})" if n > 1 else "")
        if str(level).lower() == "error":
            r.error(text)
        else:
            r.warn(text)
    r.fact(f"validate (schema, where-rules, global rules): {len(logger.statements)} statements")

    check_ifc_content(f, r)
    return r


def _unit(f, kind):
    for u in f.by_type("IfcSIUnit"):
        if u.UnitType == kind:
            return (u.Prefix or "") + u.Name
    return None


def _mesh_centroid(shape_util, geometry):
    """Centroid of a closed triangulated solid (signed tetrahedra)."""
    verts = shape_util.get_vertices(geometry)
    total, acc = 0.0, [0.0, 0.0, 0.0]
    for a, b, c in shape_util.get_faces(geometry):
        p, q, w = verts[a], verts[b], verts[c]
        v = (
            p[0] * (q[1] * w[2] - q[2] * w[1])
            - p[1] * (q[0] * w[2] - q[2] * w[0])
            + p[2] * (q[0] * w[1] - q[1] * w[0])
        ) / 6.0
        total += v
        for i in range(3):
            acc[i] += v * (p[i] + q[i] + w[i]) / 4.0
    return [x / total for x in acc]


def _distance_to_line(p, a, b):
    a = list(a) + [0.0] * (3 - len(a))
    b = list(b) + [0.0] * (3 - len(b))
    d = [b[i] - a[i] for i in range(3)]
    n = math.sqrt(sum(x * x for x in d))
    u = [x / n for x in d]
    w = [p[i] - a[i] for i in range(3)]
    t = sum(w[i] * u[i] for i in range(3))
    return math.sqrt(max(sum(x * x for x in w) - t * t, 0.0))


def check_ifc_content(f, r):
    import ifcopenshell.util.element as element
    import ifcopenshell.util.placement as placement

    ifc4 = f.schema == "IFC4"
    units = {
        "LENGTHUNIT": "METRE",
        "PLANEANGLEUNIT": "RADIAN",
        "FORCEUNIT": "NEWTON",
        "PRESSUREUNIT": "PASCAL",
        "MASSUNIT": "KILOGRAM",
    }
    projects = f.by_type("IfcProject")
    if len(projects) != 1:
        r.error(f"{len(projects)} IfcProject")
    else:
        assigned = projects[0].UnitsInContext
        if assigned is None:
            r.error("IfcProject has no units")
    for kind, name in units.items():
        got = _unit(f, kind)
        if kind in ("FORCEUNIT", "PRESSUREUNIT", "MASSUNIT") and not ifc4 and got is None:
            continue  # IFC2x3 members only: length and angle suffice
        if got != name:
            r.error(f"unit {kind}: {got}, expected {name}")

    counts = {}
    for t in [
        "IfcMember",
        "IfcProfileDef",
        "IfcMaterial",
        "IfcMaterialProfileSet",
        "IfcStructuralAnalysisModel",
        "IfcStructuralPointConnection",
        "IfcStructuralCurveMember",
        "IfcBoundaryNodeCondition",
        "IfcStructuralLoadCase",
        "IfcStructuralPointAction",
        "IfcStructuralCurveAction",
        "IfcBuildingStorey",
    ]:
        try:
            counts[t] = len(f.by_type(t))
        except RuntimeError:
            continue  # not in this schema
    r.fact("counts: " + ", ".join(f"{k} {v}" for k, v in counts.items() if v))
    profiles = f.by_type("IfcProfileDef")
    kinds = {}
    for p in profiles:
        kinds[p.is_a()] = kinds.get(p.is_a(), 0) + 1
    r.fact("profiles: " + ", ".join(f"{k} {v}" for k, v in sorted(kinds.items())))

    guids = [e.GlobalId for e in f.by_type("IfcRoot")]
    if len(set(guids)) != len(guids):
        r.error(f"GlobalIds not unique: {len(guids) - len(set(guids))} repeated")
    bad = [g for g in guids if not re.fullmatch(r"[0-9A-Za-z_$]{22}", g)]
    if bad:
        r.error(f"GlobalIds not 22 base-64 characters: {bad[:3]}")
    r.fact(f"GlobalIds: {len(guids)}, unique")

    members = f.by_type("IfcMember")
    if not members:
        r.error("no IfcMember")
    # Analysis member of each physical member (IFC4 with the analysis model).
    analysis_of = {}
    for rel in f.by_type("IfcRelAssignsToProduct"):
        for o in rel.RelatedObjects:
            if o.is_a("IfcStructuralCurveMember"):
                analysis_of[rel.RelatingProduct.id()] = o

    geom = None
    try:
        import ifcopenshell.geom
        import ifcopenshell.util.shape as shape_util

        settings = ifcopenshell.geom.settings()
        geom = (ifcopenshell.geom, shape_util, settings)
    except Exception as e:  # pragma: no cover
        r.warn(f"geometry kernel not available: {e}")

    solids = 0
    worst = 0.0
    worst_off = 0.0
    no_solid = 0
    for m in members:
        psets = element.get_psets(m)
        if ifc4 and "OHL_Member" not in psets:
            r.error(f"{m.Name}: no OHL_Member property set")
        lp = m.ObjectPlacement
        if lp is None or not lp.is_a("IfcLocalPlacement"):
            r.error(f"{m.Name}: no local placement")
            continue
        mat = placement.get_local_placement(lp)
        rot = [[mat[i][j] for j in range(3)] for i in range(3)]
        for i in range(3):
            for j in range(3):
                dot = sum(rot[k][i] * rot[k][j] for k in range(3))
                if abs(dot - (1.0 if i == j else 0.0)) > 1e-6:
                    r.error(f"{m.Name}: placement axes not orthonormal")
                    break
        det = (
            rot[0][0] * (rot[1][1] * rot[2][2] - rot[1][2] * rot[2][1])
            - rot[0][1] * (rot[1][0] * rot[2][2] - rot[1][2] * rot[2][0])
            + rot[0][2] * (rot[1][0] * rot[2][1] - rot[1][1] * rot[2][0])
        )
        if abs(det - 1.0) > 1e-6:
            r.error(f"{m.Name}: placement not right-handed (det {det:.3f})")
        reps = {rep.RepresentationIdentifier: rep for rep in m.Representation.Representations}
        axis = reps.get("Axis")
        if axis is None:
            r.error(f"{m.Name}: no Axis representation")
            continue
        line = axis.Items[0]
        ends = []
        for p in line.Points:
            c = list(p.Coordinates) + [0.0] * (3 - len(p.Coordinates))
            ends.append([sum(mat[i][j] * c[j] for j in range(3)) + mat[i][3] for i in range(3)])
        length = math.dist(ends[0], ends[1])
        if length <= 0:
            r.error(f"{m.Name}: axis of zero length")
        a = analysis_of.get(m.id())
        if a is not None:
            edge = a.Representation.Representations[0].Items[0]
            joints = [
                list(edge.EdgeStart.VertexGeometry.Coordinates),
                list(edge.EdgeEnd.VertexGeometry.Coordinates),
            ]
            if max(math.dist(ends[0], joints[0]), math.dist(ends[1], joints[1])) > 1e-6:
                r.error(f"{m.Name}: axis ends {ends} not at its analysis member's joints {joints}")
        body = reps.get("Body")
        if body is None:
            no_solid += 1
            continue
        if geom is None:
            continue
        g, shape_util, settings = geom
        try:
            shape = g.create_shape(settings, m)
        except Exception as e:
            r.error(f"{m.Name}: geometry kernel could not build the body: {e}")
            continue
        volume = shape_util.get_volume(shape.geometry)
        if not volume > 0:
            r.error(f"{m.Name}: body volume {volume}")
            continue
        solids += 1
        # The solid's centroid (local coordinates, from the triangulated
        # mesh) on the member's axis: the profile is placed at its centroid.
        c = _mesh_centroid(shape_util, shape.geometry)
        axis_local = [list(p.Coordinates) for p in line.Points]
        off = _distance_to_line(c, axis_local[0], axis_local[1])
        worst_off = max(worst_off, off)
        if off > 1e-3:
            r.warn(f"{m.Name}: body centroid {off * 1000:.2f} mm off the member's axis")
        if ifc4:
            usage = [
                rel.RelatingMaterial
                for rel in m.HasAssociations
                if rel.is_a("IfcRelAssociatesMaterial")
            ]
            area = None
            if usage and usage[0].is_a("IfcMaterialProfileSetUsage"):
                prof = usage[0].ForProfileSet.MaterialProfiles[0].Profile
                for pp in prof.HasProperties or []:
                    for prop in pp.Properties:
                        if prop.Name == "CrossSectionArea":
                            area = prop.NominalValue.wrappedValue
            if area:
                ratio = volume / (area * length)
                worst = max(worst, abs(ratio - 1.0))
                if abs(ratio - 1.0) > 0.03:
                    r.warn(
                        f"{m.Name}: body volume / (A x L) = {ratio:.4f} "
                        f"(profile {prof.ProfileName}, {prof.is_a()})"
                    )
    r.fact(
        f"members: {len(members)} placements checked, {solids} body solids built"
        + (f", {no_solid} without a body (axis only)" if no_solid else "")
        + (f", worst |volume / (A x L) - 1| = {worst:.4f}" if ifc4 and solids else "")
        + (f", centroid at most {worst_off * 1000:.3f} mm off the axis" if solids else "")
    )
    if ifc4:
        for p in profiles:
            if p.is_a() == "IfcProfileDef":
                continue  # by name only, no geometry
            names = {pp.Name for pp in (p.HasProperties or [])}
            if "OHL_Section" not in names:
                r.error(f"profile {p.ProfileName}: no OHL_Section")


# ---------------------------------------------------------------- SAF

NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
REL_NS = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id"


def _columns_from_saf_rs():
    """The exporter's own column lists (saf.rs `pub mod columns`)."""
    here = os.path.dirname(os.path.abspath(__file__))
    src = os.path.join(here, "..", "apps", "structure", "ohl-structure-export", "src", "saf.rs")
    text = open(src, encoding="utf-8").read()
    version = re.search(r'pub const SAF_VERSION: &str = "([^"]+)"', text).group(1)
    consts = dict(
        (name, re.findall(r'"([^"]*)"', body))
        for name, body in re.findall(r"pub const (\w+): &\[&str\] = &\[(.*?)\];", text, re.S)
    )
    sheets = {
        "StructuralMaterial": "MATERIAL",
        "StructuralCrossSection": "CROSS_SECTION",
        "CompositeShapeDef": "COMPOSITE_SHAPE",
        "StructuralPointConnection": "POINT_CONNECTION",
        "StructuralCurveMember": "CURVE_MEMBER",
        "StructuralPointSupport": "POINT_SUPPORT",
        "StructuralLoadGroup": "LOAD_GROUP",
        "StructuralLoadCase": "LOAD_CASE",
        "StructuralPointAction": "POINT_ACTION",
        "StructuralCurveAction": "CURVE_ACTION",
    }
    return version, {s: consts[c] for s, c in sheets.items()}


def _cell_ref(ref):
    letters = re.match(r"[A-Z]+", ref).group(0)
    col = 0
    for ch in letters:
        col = col * 26 + ord(ch) - 64
    return col - 1


def read_workbook(path):
    """{sheet: [[value, ...], ...]} with strings and floats (None: empty)."""
    z = zipfile.ZipFile(path)
    bad = z.testzip()
    if bad:
        raise ValueError(f"corrupt zip member {bad}")
    names = set(z.namelist())
    for part in ["[Content_Types].xml", "_rels/.rels", "xl/workbook.xml"]:
        if part not in names:
            raise ValueError(f"missing part {part}")
    shared = []
    if "xl/sharedStrings.xml" in names:
        root = ET.fromstring(z.read("xl/sharedStrings.xml"))
        for si in root.findall("m:si", NS):
            shared.append("".join(t.text or "" for t in si.iter(f"{{{NS['m']}}}t")))
    rels = ET.fromstring(z.read("xl/_rels/workbook.xml.rels"))
    target = {
        rel.get("Id"): rel.get("Target") for rel in rels
    }
    wb = ET.fromstring(z.read("xl/workbook.xml"))
    out = {}
    for sh in wb.find("m:sheets", NS):
        name = sh.get("name")
        t = target[sh.get(REL_NS)]
        part = t.lstrip("/") if t.startswith("/") else "xl/" + t
        root = ET.fromstring(z.read(part))
        rows = []
        for row in root.iter(f"{{{NS['m']}}}row"):
            vals = []
            for c in row.findall("m:c", NS):
                k = _cell_ref(c.get("r")) if c.get("r") else len(vals)
                while len(vals) < k:
                    vals.append(None)
                t = c.get("t")
                if t == "inlineStr":
                    v = "".join(x.text or "" for x in c.iter(f"{{{NS['m']}}}t"))
                elif t == "s":
                    v = shared[int(c.find("m:v", NS).text)]
                else:
                    e = c.find("m:v", NS)
                    v = None if e is None else float(e.text)
                vals.append(v)
            rows.append(vals)
        out[name] = rows
    return out


def check_saf(path):
    r = Report(path)
    try:
        book = read_workbook(path)
    except Exception as e:
        r.error(f"workbook does not open: {e}")
        return r
    version, columns = _columns_from_saf_rs()
    r.fact("sheets: " + ", ".join(f"{k} {max(len(v) - 1, 0)}" for k, v in book.items()))

    def table(name):
        rows = book.get(name)
        if not rows:
            return []
        head = rows[0]
        return [dict(zip(head, row)) for row in rows[1:]]

    for required in [
        "Project",
        "Model",
        "StructuralMaterial",
        "StructuralCrossSection",
        "StructuralPointConnection",
        "StructuralCurveMember",
    ]:
        if required not in book:
            r.error(f"sheet {required} missing")
    for name in book:
        if name in ("Project", "Model"):
            continue
        if name not in columns:
            r.error(f"sheet {name} not one the exporter writes")
            continue
        head = book[name][0]
        while head and head[-1] is None:
            head = head[:-1]
        if head != columns[name]:
            r.error(f"{name}: columns {head} differ from saf.rs {columns[name]}")
        ids = [row.get("Name") for row in table(name)]
        if len(set(ids)) != len(ids):
            r.error(f"{name}: names not unique")
        uids = [row.get("Id") for row in table(name)]
        if any(not re.fullmatch(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", str(u)) for u in uids):
            r.error(f"{name}: an Id is not a UUID")
    model = {row[0]: row[1] if len(row) > 1 else None for row in book.get("Model", [])}
    if model.get("SAF Version") != version:
        r.error(f"Model: SAF Version {model.get('SAF Version')}, expected {version}")
    if model.get("System of units") != "Metric":
        r.error(f"Model: System of units {model.get('System of units')}")
    r.fact(f"Model: SAF Version {model.get('SAF Version')}, units {model.get('System of units')}")

    materials = {m["Name"]: m for m in table("StructuralMaterial")}
    shapes = {s["Name"] for s in table("CompositeShapeDef")}
    sections = {s["Name"]: s for s in table("StructuralCrossSection")}
    nodes = {n["Name"] for n in table("StructuralPointConnection")}
    members = {m["Name"]: m for m in table("StructuralCurveMember")}
    groups = {g["Name"] for g in table("StructuralLoadGroup")}
    cases = {c["Name"] for c in table("StructuralLoadCase")}

    def number(sheet, name, row, col, positive=False):
        v = row.get(col)
        if not isinstance(v, float) or math.isnan(v) or math.isinf(v):
            r.error(f"{sheet} {name}: {col} is not a number ({v!r})")
        elif positive and v <= 0:
            r.error(f"{sheet} {name}: {col} = {v}, expected > 0")

    for name, m in materials.items():
        for col in ["Unit mass [kg/m3]", "E modulus [MPa]", "G modulus [MPa]"]:
            number("StructuralMaterial", name, m, col, positive=True)
        e = m.get("E modulus [MPa]")
        if isinstance(e, float) and not 1e2 < e < 1e6:
            r.warn(f"material {name}: E = {e} MPa (outside 100 MPa..1000 GPa: units?)")
    for name, s in sections.items():
        if s.get("Material") not in materials:
            r.error(f"cross-section {name}: material {s.get('Material')} not in StructuralMaterial")
        kind = s.get("Cross-section type")
        if kind == "Parametric":
            if not s.get("Shape") or not s.get("Parameters [mm]"):
                r.error(f"cross-section {name}: Parametric without Shape or Parameters")
            else:
                for p in str(s["Parameters [mm]"]).split(";"):
                    try:
                        float(p)
                    except ValueError:
                        r.error(f"cross-section {name}: parameter {p!r} not a number")
        elif kind == "General":
            if s.get("Profile") not in shapes:
                r.error(f"cross-section {name}: profile {s.get('Profile')} not in CompositeShapeDef")
        else:
            r.error(f"cross-section {name}: type {kind!r}")
        number("StructuralCrossSection", name, s, "A [m2]", positive=True)
        zero = []
        for col in ["Iy [m4]", "Iz [m4]", "It [m4]"]:
            number("StructuralCrossSection", name, s, col)
            if s.get(col) == 0.0:
                zero.append(col.split(" ")[0])
        if zero:
            # A [[section]] table may give the area only (truss members);
            # an analysis program may refuse zero stiffness for a frame.
            r.warn(f"cross-section {name}: {', '.join(zero)} = 0 (the section gives no value)")
        a = s.get("A [m2]")
        if isinstance(a, float) and not 1e-7 < a < 1.0:
            r.warn(f"cross-section {name}: A = {a} m2 (units?)")
    for s in table("CompositeShapeDef"):
        for k in ("1", "2"):
            mat = s.get(f"Material name {k}")
            if mat is not None and mat not in materials:
                r.error(f"CompositeShapeDef {s['Name']}: material {mat} unknown")
        for k in ("1", "2"):
            poly = s.get(f"Polygon contour {k}")
            if poly is None:
                continue
            pts = [tuple(float(x) for x in p.split(";")) for p in str(poly).split("|")]
            area = 0.5 * sum(
                pts[i][0] * pts[(i + 1) % len(pts)][1] - pts[(i + 1) % len(pts)][0] * pts[i][1]
                for i in range(len(pts))
            )
            if (k == "1") != (area > 0):
                r.error(f"CompositeShapeDef {s['Name']}: contour {k} turns the wrong way")
    for name, m in members.items():
        if m.get("Cross section") not in sections:
            r.error(f"member {name}: cross-section {m.get('Cross section')} unknown")
        ends = [x.strip() for x in str(m.get("Nodes")).split(";")]
        if len(ends) != 2 or any(e not in nodes for e in ends):
            r.error(f"member {name}: nodes {m.get('Nodes')} unknown")
        number("StructuralCurveMember", name, m, "Length [m]", positive=True)
        z = [m.get(f"Coordinate {c} [m]") for c in "XYZ"]
        if all(isinstance(v, float) for v in z):
            if abs(math.hypot(*z) - 1.0) > 1e-6:
                r.error(f"member {name}: LCS vector not a unit vector")
        else:
            r.error(f"member {name}: LCS vector missing")
    for s in table("StructuralPointSupport"):
        if s.get("Node") not in nodes:
            r.error(f"support {s.get('Name')}: node {s.get('Node')} unknown")
        for d in ["ux", "uy", "uz", "fix", "fiy", "fiz"]:
            if s.get(d) not in ("Rigid", "Free", "Flexible"):
                r.error(f"support {s.get('Name')}: {d} = {s.get(d)!r}")
    for c in table("StructuralLoadCase"):
        if c.get("Load group") not in groups:
            r.error(f"load case {c.get('Name')}: load group {c.get('Load group')} unknown")
    for a in table("StructuralPointAction"):
        if a.get("Load case") not in cases:
            r.error(f"point action {a.get('Name')}: load case unknown")
        if a.get("Force action") == "In node" and a.get("Reference node") not in nodes:
            r.error(f"point action {a.get('Name')}: node {a.get('Reference node')} unknown")
        if a.get("Force action") == "On beam":
            if a.get("Reference member") not in members:
                r.error(f"point action {a.get('Name')}: member unknown")
            x = a.get("Position x [m]")
            if a.get("Coordinate definition") == "Relative" and not (
                isinstance(x, float) and 0.0 <= x <= 1.0
            ):
                r.error(f"point action {a.get('Name')}: relative position {x!r}")
        number("StructuralPointAction", a.get("Name"), a, "Value [kN]")
    for a in table("StructuralCurveAction"):
        if a.get("Load case") not in cases:
            r.error(f"curve action {a.get('Name')}: load case unknown")
        if a.get("Member") not in members:
            r.error(f"curve action {a.get('Name')}: member {a.get('Member')} unknown")
        number("StructuralCurveAction", a.get("Name"), a, "Value 1 [kN/m]")
    r.fact(
        f"references: {len(members)} members -> {len(sections)} cross-sections -> "
        f"{len(materials)} materials, {len(nodes)} joints, {len(cases)} load cases"
    )
    return r


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    files = []
    for a in argv:
        if os.path.isdir(a):
            for root, _, names in os.walk(a):
                files += [os.path.join(root, n) for n in sorted(names) if n.endswith((".ifc", ".xlsx"))]
        else:
            files.append(a)
    failed = 0
    for p in files:
        r = check_ifc(p) if p.endswith(".ifc") else check_saf(p)
        r.print()
        failed += bool(r.errors)
    print(f"{len(files)} files checked, {failed} with errors")
    return 1 if failed or not files else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
