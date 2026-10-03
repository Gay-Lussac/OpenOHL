# Coordinate systems

Lines, surveys and point clouds are in a projected coordinate system in
metres (the line file's `crs`, e.g. `"EPSG:2056"`). OLS CAD knows the EPSG
registry and proj4 strings, and transforms points and whole lines between
systems — pure Rust, no native library, the same in plugins and the future
web client.

```bash
ols crs info EPSG:2056
ols crs transform --from EPSG:2056 --to EPSG:4326 2600000 1200000
# 7.438632421 46.951082772
ols crs line line.ols.json --to EPSG:32632 --out line-utm.ols.json
```

- **Geographic** coordinates are longitude, latitude in degrees.
- **Heights are kept** by default: line, survey and LiDAR heights are
  usually orthometric (above a geoid) and no geoid model is applied.
  `--ellipsoidal` treats them as ellipsoidal heights through the datum
  shift (`--points 3` to give X Y Z).
- **A whole line** (`ols crs line`): structure positions, attachment, leg
  and stick points, alignments and jumper supports move together; structure
  orientations follow the rotation between the two grids. Lines stay in
  projected systems. **Grid distances change** with each projection's scale
  (a span of 82.206 m in LV95 is 82.183 m in UTM 32N): check spans and
  stringing after moving a line.

## Accuracy

Datum changes use the Helmert parameters of the definitions, not grid
files.

- **Switzerland, LV95 (EPSG:2056) ↔ WGS84**: matches swisstopo's official
  transformation service REFRAME to under 1 mm in position and height
  (validation cases `crs-001` … `crs-012`), with WGS84 taken as
  CHTRS95/ETRS89 as REFRAME does.
- **Projection changes within one datum** (e.g. WGS84 ↔ UTM) are exact to
  the millimetre (validation case `crs-013`).
- **Datums that need distortion grids** (LV03 ↔ LV95, NAD27 ↔ NAD83, …) are
  approximated by their Helmert parameters; a PROJ-based plugin with grid
  files can provide them behind the same interface.
- With heights kept, a round trip through another datum returns within
  about 1 mm (the datum shift's height change is dropped each way).
