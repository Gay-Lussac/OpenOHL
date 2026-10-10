# OLS XML schema

OLS XML is the whole line model — the content of a [line file](../user-guide/line-file.md) (`.ols.json`) — as XML, for tools that prefer XML. It holds the same data: a file written by OLS CAD and read back gives the same line, number for number.

- **Namespace:** `urn:ols-cad:ols-line:1`
- **Schema:** [`docs/schema/ols-line.xsd`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/docs/schema/ols-line.xsd) (XML Schema 1.0)
- **Write:** `ols line LINE --export line.xml` (format `ols-xml`, the default for `.xml`) or File › Export… in the application.
- **Read:** `ols import line.xml --out line.ols.json`, `ols project import-file line.xml --into p.ols --user ann`, or File › Import into the line… in the application. The reader also recognises LandXML files (root `LandXML`, see [Import and export](../user-guide/import-export.md#landxml)).

## Rules

The names are those of the line file:

- An object is an element; its plain values are **attributes** named like the JSON keys (`orientation_deg`, `phase_label`, `voltage_kv`…).
- A point is three attributes `x`, `y`, `z` (metres, the line's coordinate system).
- A list is **repeated child elements** in order (`<structure>`, `<section>`, `<point>`…); number lists (span lengths, stick drawing polylines) are space-separated text.
- An absent optional value is an absent attribute; defaults are those of the line file (bundle 1, jumper temperature 15 °C, calculation `initial-fe`…).
- Numbers are written as the shortest decimal text that reads back to the same value.
- Unknown elements and attributes are refused (as unknown keys are in the line file).

## Elements

```xml
<line xmlns="urn:ols-cad:ols-line:1" format="ols-line" version="1" name="…" crs="EPSG:2056">
  <alignment>                                   <!-- implicit route only -->
    <point x="…" y="…" z="…" structure="T1"/>   <!-- structure: tied point -->
  </alignment>
  <route id="main" name="Main route">
    <alignment>…</alignment>
    <structure ref="T1"/> …
  </route>
  <structure id="T1" x="…" y="…" z="…" orientation_deg="-90" type="…" extension="…">
    <attachment set="1" phase="1" label="…" insulator_weight="…" insulator="…"
                insulator_wind_area="…" top_label="…" bottom_label="…" strain_member="true">
      <insulator_point x="…" y="…" z="…"/>
      <wire_point x="…" y="…" z="…"/>
      <leg …/><leg …/>                          <!-- V-strings -->
    </attachment>
    <omit set="2" phase="1"/>
    <member>x y z x y z …</member>              <!-- stick drawing polyline -->
  </structure>
  <system id="…" name="…" kind="ac" voltage_kv="220" frequency_hz="50">
    <phase label="L1" angle_deg="0" polarity="1"/>
  </system>
  <section id="S1" cable="…" circuit="…" phase_label="L1" system="…" bundle="1"
           bundle_spacing="…" bundle_rotation_deg="…" color="#rrggbb" calculation="creep-rs">
    <point structure="T1" set="1" phase="1"/> …
    <stringing kind="tension" horizontal_tension="25000" temperature="15"/>
    <!-- or kind="sag" sag="…" temperature="…" span="…" -->
    <!-- or kind="unstressed_lengths" temperature="…">
           <lengths>300.1 299.9 …</lengths><stringing kind="sag" …/></stringing> -->
    <load span="1" fraction="0.5" vertical="55" label="marker ball"/>
  </section>
  <jumper id="…" cable="…" length="…" sag="…" temperature="15" bending_stiffness="…" stiffness_factor="…"
          angle_from_deg="…" angle_to_deg="…" color="…" bundle="…" bundle_spacing="…"
          bundle_rotation_deg="…">
    <from structure="T3" set="2" phase="1"/>
    <to structure="T3" set="1" phase="1"/>
    <support length="…" weight="…" fraction="0.5" x="…" y="…" z="…"/>
  </jumper>
  <circuit_start structure="T7" set="1" phase="1"/>  <!-- a circuit's end that is a start -->
  <phase_color label="L1" color="#8b4513"/>     <!-- display colours -->
  <creep_stretch creep_case="…">
    <load_case name="…"/>
  </creep_stretch>
  <bimetallic behavior="bird_cage"/>            <!-- or compression, max_virtual_compression_pa="…" -->
</line>
```

The meaning of every value is that of the line file ([Line files](../user-guide/line-file.md)). Like a line file, an OLS XML line is checked (references, stringing, alignment ties) when it is read.
