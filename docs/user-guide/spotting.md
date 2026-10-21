# Optimum structure spotting

**Spotting** places the structures of a route: where they stand, which type and which extension. OpenOHL finds the placement that **costs least** while every span keeps its ground clearance and every structure stays within its allowable spans. The result is a proposal — a new line file — to check and refine; the model you work on is not changed.

How it works is in the [theory manual](../theory/spotting.md).

## What it needs

- **A route** of the line: its alignment (or the polyline through its structures). Stations are measured along it; its inner points that turn are **alignment points**, where a structure must stand.
- **The ground**: a terrain surface (survey points). The profile under the route's centreline is used; holes in the surface are bridged in a straight line (and reported).
- **Structure types** (`.ohls`) and a **cost file** (`.ohlcost`, see [Bill of materials](bom-and-cost.md)): every type and extension with a price in the cost file is a candidate, at its supply + foundation + erection price. A candidate's wire height is its lowest wire point (the clamp of a suspension string, the attachment of a strain string). A type whose attachments are all strain points is a **dead-end**: only dead-ends stand at the start and the end of the route.
- **Allowable spans** (`[[capacity.spans]]` of the types, Method 1): a type may stand at a line angle only up to its last given angle; its wind span and weight span must stay within the values interpolated at its angle. A type without allowable spans has no span limits, and only a dead-end of that kind may turn.
- **The wire**: a wire type and its **horizontal tension in the design (maximum sag) condition**, with the wire's own weight per metre. Each span is a catenary of that tension between the wire heights of its two structures.
- **The clearance**: the smallest vertical distance from that wire to the ground.
- **The step** between candidate stations, and optionally the longest and shortest span and zones where no structure may stand.

Costs: the structures (from the cost file), each span's stringing labour (`labour.per_span`), and the wire (the catenary length of each span × the wire type's price and stringing per metre × the number of wires, by default the fewest attachments of a candidate type).

## From the command line

```bash
ohl spot examples/synthetic-line/synthetic-line-typed.ohl.json --route main \
    --terrain examples/synthetic-line/synthetic-survey.csv \
    --types examples/synthetic-line/Structures \
    --cost examples/synthetic-line/unit-costs.ohlcost \
    --cable acsr-drake-26-7 --tension 20 --clearance 7 --step 10 --max-span 450 \
    --out spotted.ohl.json
```

| Option | Default | Notes |
|---|---|---|
| `--route` | the first route | Route id (`ohl line FILE --stations` lists them). |
| `--terrain` | | Survey points (CSV / XYZ). |
| `--types` | `Structures/` next to the line | Candidates: the types the cost file prices. |
| `--insulators`, `--library` | shipped libraries | Insulator assemblies, wire library. |
| `--cost` | | Unit costs. |
| `--cable`, `--tension` | | Wire type id; horizontal tension in kN in the design condition. |
| `--temperature` | 80 °C | Temperature of the design condition: the sections of the new line are strung to `--tension` at it, in still air. |
| `--clearance` | | Vertical ground clearance, m. |
| `--step` | 10 m | Distance between candidate stations. |
| `--max-span`, `--min-span` | twice the largest allowable wind span; 0 | Span limits, m. |
| `--forbid FROM:TO` | | No structure between these stations (repeatable). |
| `--conductors` | the fewest attachments of a candidate | Wires per span priced per metre. |
| `--seconds` | 300 | Stops after this long (and says so). |
| `--out` | | Writes the proposal as a line file. |

It prints one row per structure (station, type, extension, line angle, span ahead and its smallest clearance) and the costs:

```
     #  station m  type         ext    angle°    span m   clear. m         cost
     1        0.0  type-06                0.0     320.0      11.69     39500.00
     2      320.0  type-05                0.0     320.0       7.59     19500.00
     3      640.0  type-03              -10.0     300.0      15.20     39000.00
     4      940.0  type-05                0.0     320.0       9.04     19500.00
     5     1260.0  type-06                0.0                          39500.00
Structures 157000.00 + spans 6000.00 + wire 9223.47 = 172223.47 EUR
```

**The new line** has the route (its centreline as free alignment points), the structures `T1`, `T2`, … (typed, with their extension, on the ground, turned to the route — the bisector at alignment points), and one section for every attachment (set, phase) all chosen types have, strung to the design tension at the design temperature. The rest of the original line (its other routes, structures and sections) is not carried over; its name (with "(spotted)") and coordinate system are. Phases and electrical systems are for you to assign. Open it in the application or check it with `ohl line spotted.ohl.json --terrain … --clearance …`.

## In the application

Structures › **Spot structures…** asks for the route, wire type, design tension and temperature, clearance, step, longest span and cost file (the project's first `.ohlcost` by default; next to a line file). It uses the model's terrain surface and structure types. **Spot** runs in the background (**Cancel** stops it); the proposal is listed with its costs, and **Save as line file…** writes it as a new line.

## Limits of this first version

- The clearance is checked under the centreline, to the lowest wire, in one design condition (a catenary of the given tension, no wind, no string swing); conductors aside of the centreline over sloping ground across the line are not checked — check the proposal with the clearance checks.
- One tension for the whole route (no tension sections broken at dead-ends).
- Weight spans use the same design condition (normally the cold, heavy case governs uplift).
- No crossing constraints (roads, other lines) besides forbidden zones.
