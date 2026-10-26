# Bill of materials and cost estimate (`.ohlcost`)

The **bill of materials** lists what a line is built from: structures by type and extension, insulator assemblies, wire lengths by wire type, and hardware counted by rule. A **cost file** (`.ohlcost`) prices it; items without a price are listed so nothing is silently left out.

## What is counted

| Item | How |
|---|---|
| Structures | By type and extension (structures given by their strings only are listed as "(no type)"). |
| Insulator assemblies | Every string holding a wire point a section is strung through (a bundle hangs from one; strings shared by sections counted once), by insulator assembly id. Strings given in a structure file or only as geometry are listed by kind and length, e.g. `string suspension 3.500 m`, `string strain 5.025 m` (a V: two strings). A wire clamped straight to the structure has none. |
| Wire | Per wire type and role — `conductor`, `earth wire` (sections whose conductor path has no electrical system), `wire` (a line without electrical systems), `jumper`. The **unstressed (cut) length** of every span at the section's stringing condition: the given lengths of a section clipped in; for a section in the blocks, the lengths that give its tension or sag (the same calculation that clips it in). Times the subconductors of the bundle. If a section cannot be solved, its chords are used and a note says so. |
| Jumpers | Their cut length if given, else the chord between the clamps plus the parabolic slack \\( 8 d^2 / 3 c \\) for the sag \\( d \\) over the chord \\( c \\); times the jumper's bundle. |
| Spacers | Bundles only: along each span one spacer every `spacer_interval` (\\( \lceil L / s \rceil - 1 \\) per span). |
| Dampers | `dampers_per_span` per subconductor and span, every section. |
| Spans | Spans between two structures (pairs of structures next to each other in a section), for the stringing labour. |

No sag or waste allowance is added to the wire lengths: add one to the price per metre if you use it.

## Cost files (`.ohlcost`)

```toml
format = "ohl-cost"
version = 1
name = "Example unit costs (invented)"
currency = "EUR"           # a label; nothing is converted

[labour]
per_structure = 2500.0     # erecting a structure (unless its type gives its own)
per_span = 1800.0          # stringing, per span between two structures

[[structure]]
type = "type-02"
price = 38000.0            # supply, without extension
foundation = 9000.0        # per structure (optional)
erection = 4000.0          # replaces labour.per_structure (optional)
extensions = { "+3" = 41000.0 }   # supply price with each extension

[[insulator]]
id = "I-220-single"        # an insulator assembly id, or an inline string's name
price = 650.0

[[cable]]
id = "acsr-drake-26-7"
price_per_m = 7.5
stringing_per_m = 1.2      # optional

[hardware]
spacer_interval = "60 m"   # bundles: a spacer every … along each span
spacer_price = 45.0
dampers_per_span = 2       # per subconductor and span
damper_price = 60.0

[[fixed]]
item = "Site establishment"
price = 25000.0
```

| Section | Field | Notes |
|---|---|---|
| top | `currency`, `name` | Labels. |
| `[labour]` | `per_structure`, `per_span` | Optional. |
| `[[structure]]` | `type`, `price`, `extensions`, `foundation`, `erection` | A structure with an extension is priced from `extensions` only. A type or extension without a supply price is listed as missing. |
| `[[insulator]]` | `id`, `price` | Per assembly. |
| `[[cable]]` | `id`, `price_per_m`, `stringing_per_m` | Per metre of wire (every subconductor). |
| `[hardware]` | `spacer_interval`, `spacer_price`, `dampers_per_span`, `damper_price` | The rules count only when given. |
| `[[fixed]]` | `item`, `price` | Added once. |

Prices are numbers ≥ 0; the same type, insulator or wire type may be priced once. The cost data is yours: OpenOHL ships only `examples/synthetic-line/unit-costs.ohlcost`, whose prices are invented.

## In the application

Plans & sheets › **Bill of materials › Wires…** opens a window with the wires' part of the bill — the categories `wire` and `stringing` — its total and its items without a price. Structures, insulators and hardware are counted by `ohl bom` only: the application does not offer them yet. The project's first `.ohlcost` file (copied in from next to the project when it opens), or the first next to a line file, is used; **Choose…** picks another, **No prices** shows quantities only. It is computed in the background from the line as it is (the model is not changed); **Run** (the window's footer, bottom left) after edits. **Export CSV…** writes the table.

## From the command line

```bash
ohl bom examples/synthetic-line/synthetic-line-typed.ohl.json \
    --cost examples/synthetic-line/unit-costs.ohlcost --csv bom.csv --json bom.json
```

`--library`, `--structures` and `--insulators` choose the libraries as for `ohl line`. Without `--cost` the quantities are listed without prices.

The CSV columns: `category,item,quantity,unit,unit_price,amount,currency`, one row per line of the table and a `total` row when priced. Categories: `structures`, `foundations`, `erection`, `insulators`, `wire`, `stringing`, `hardware`, `fixed`. The JSON holds the whole bill (also the cut length of every span of every section) and the estimate.
