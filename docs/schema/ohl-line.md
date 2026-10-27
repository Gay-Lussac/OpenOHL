# OHL XML schema

OHL XML is the whole line model — the content of a [line file](../user-guide/line-file.md) (`.ohl.json`) — as XML, for tools that prefer XML. It holds the same data: a file written by OpenOHL and read back gives the same line, number for number.

- **Namespace:** `urn:openohl:ohl-line:1`.
- **Schema:** [`docs/schema/ohl-line.xsd`](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/schema/ohl-line.xsd) (XML Schema 1.0)
- **Write:** `ohl line LINE --export line.xml` (format `ohl-xml`, the default for `.xml`) or File › Export › The line… in the application.
- **Read:** `ohl import line.xml --out line.ohl.json`, `ohl project import-file line.xml --into p.ohl --user ann`, or File › Import › Into the line… in the application. The reader also recognises LandXML files (root `LandXML`, see [Import and export](../user-guide/import-export.md#landxml)).

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
<line xmlns="urn:openohl:ohl-line:1" format="ohl-line" version="7" name="…" crs="EPSG:2056"
      height_datum="LN02" height_datum_epsg="5728">  <!-- version 7: the heights' datum, optional -->
  <route id="main" name="Main route" description="…"/>
  <alignment id="alignment 1" name="…" route="main">  <!-- one route -->
    <point ref="P1"/> <point ref="P2"/> …       <!-- its points, in order -->
  </alignment>
  <alignment_point id="P1" x="…" y="…" z="…" structure="T1"/> <!-- structure: tied -->
  <!-- older files: version 3 <alignment> with <on_route id/> and <point x y z>
       inside; version 2 <route> holding its <alignment> and <structure ref>,
       and the line's own <alignment> without id: read and converted -->
  <structure id="T1" x="…" y="…" z="…" orientation_deg="-90" type="…" extension="…"
             height_adjust="0.5">                <!-- feet 0.5 m above the ground -->
    <weather_group name="…"/>                   <!-- its weather case groups (the structure override) -->
    <point label="A1" x="…" y="…" z="…"/>       <!-- where strings hang -->
    <wire label="W1" x="…" y="…" z="…" set="1" weight="…"/> <!-- set: a wire ends here; weight: a counterweight -->
    <string label="S1" top="A1" wire="W1" insulator="…" length="…" angle_deg="…" direction_deg="…" weight="…" wind_area="…"
            stiffness="…" compression="…" strain="true"
            max_load="120000" max_swing_deg="60"
            max_swing_across_deg="55" max_swing_along_deg="30"/>  <!-- top: a point or a wire; max_*: limits not checked (version 6) -->
    <string label="S2" top="A2" wire="W2" insulator="…">  <!-- version 8: its own volumes, over its assembly's by name -->
      <shape name="clamp" kind="box" state="electrified" at_fraction="1" width="0.15" depth="0.3" length="0.1"/>
    </string>
    <omit label="…"/> <omit set="2"/>           <!-- typed: the type's left out -->
    <!-- files of version 1: <attachment set phase …> with <insulator_point>,
         <wire_point>, <leg>; <omit set phase>; section points with a phase -->
    <member>x y z x y z …</member>              <!-- stick drawing polyline -->
  </structure>
  <system id="…" name="…" owner="…" number="…" kind="ac" voltage_kv="220" frequency_hz="50" color="#e4572e">  <!-- owner, number: the phase plan's table -->
    <phase label="L1" angle_deg="0" polarity="1" code=".-"/>  <!-- code: line code by system -->
  </system>
  <circuit id="…" name="North" system="…" description="…"/>  <!-- version 7: a circuit; its sections give its name -->
  <section id="S1" cable="…" circuit_name="…" phase_label="L1" system="…" bundle="1"
           bundle_spacing="…" bundle_rotation_deg="…" color="#rrggbb" calculation="creep-rs">
    <weather_group name="…"/>                   <!-- its weather case groups (the section override) -->
    <wind_factor span="2" factor="1.2"/>        <!-- version 9: a local wind, span 0 = the first -->
    <point structure="T1" set="1"/> …
    <stringing kind="tension" horizontal_tension="25000" temperature="15"/>
    <!-- or kind="sag" sag="…" temperature="…" span="…" -->
    <!-- or kind="unstressed_lengths" temperature="…">
           <lengths>300.1 299.9 …</lengths><stringing kind="sag" …/></stringing> -->
    <span_point label="M1" span="1" fraction="0.5" description="marker ball">  <!-- or distance="…" or station="…" -->
      <object kind="body" type="marker-ball-600" weight="55" shape="sphere" diameter="0.6" drag="1"/>
    </span_point>
    <span_point label="C1" span="0" distance="40">
      <object kind="clamp" weight="30" shape="point" clamp_length="0.25"/>  <!-- a tap's or a spacer's clamp -->
    </span_point>
    <span_point label="P1" span="2" fraction="0.2"/>   <!-- bare: a spread's end -->
    <span_point label="P2" span="2" fraction="0.4"/>
    <spread label="heated" from="P1" to="P2" type="heated-stretch" load="12.5"/>  <!-- N/m, the whole bundle's;
                                                  per_subconductor="true": on each subconductor -->
  </section>
  <jumper id="…" cable="…" length="…" sag="…" tension="…" temperature="15" bending_stiffness="…" stiffness_factor="…"
          angle_from_deg="…" angle_to_deg="…" color="…" bundle="…" bundle_spacing="…"
          bundle_rotation_deg="…">
    <from structure="T3" set="2"/>               <!-- or <from_tap section="S1" point="C1"/>, <from_tap jumper="J2" point="K"/> -->
    <to structure="T3" set="1"/>                 <!-- or <to_tap …/> -->
    <jumper_point label="C1" fraction="0.5">     <!-- or length="…" (along it) or distance="…" (from its from clamp) -->
      <object kind="clamp" clamp_length="0.1">   <!-- held up: a support -->
        <hold structure="T3" wire="JW"/>         <!-- a structure's strings; or <hold x y z length weight/>: a string from a point -->
      </object>
    </jumper_point>
  </jumper>
  <!-- structures, sections, jumpers, systems, alignments and routes may
       hold the values of plugins' table columns, last inside them:
       <field key="code.insulation_level" value="low"/> -->
  <!-- files before version 5, read and converted: in <section>, <load span fraction|distance|station vertical
       label diameter drag wind_area ice_area/> (a point with a body) and <spread_load span start_… end_… load
       label/> (two points and a spread); in <jumper>, <support length weight fraction x y z/> and <clamp structure
       wire at clamp_length/> (support points), a placeholder <from> beside <from_tap>; several of fraction,
       distance, station on one place (a station over a distance over the fraction); circuit= for circuit_name= -->
  <spacer id="SP1" weight="120" compression="10000">   <!-- an interphase spacer -->
    <end section="S1" point="P1"/> <end section="S2" point="P1"/>
  </spacer>
  <circuit_start structure="T7" set="1"/>       <!-- a circuit's end that is a start -->
  <phase_color label="L1" color="#8b4513"/>     <!-- display colours -->
  <creep_stretch creep_case="…">
    <load_case name="…"/>
  </creep_stretch>
  <defaults spacer_compression="10000"/>        <!-- the project's default values -->
  <solver jumper_loads="pull" taps="once" loads_per="unstretched" blocks="equal_h"/>  <!-- Criteria › Calculation options; defaults estimate, loop, stretched, sheaves (blocks: version 10) -->
  <bimetallic behavior="bird_cage"/>            <!-- or compression, max_virtual_compression_pa="…" -->
  <structure_loads condition="creep">           <!-- Criteria › Loads for structures; default as strung, no weather case -->
    <weather_case name="…"/>                    <!-- a wind case sent twice (wind from the left, from the right); load_case read, not used since 2026-10-25 -->
    <broken_case name="…"/>                     <!-- version 9: a load case with a broken wire, its weather case with the wire broken -->
  </structure_loads>
  <electrical soil_resistivity_ohm_m="100" earth_return="carson_series" mean_height="solved"/>  <!-- version 11: line parameters; earth_return also complex_depth, carson_first_terms; mean_height also two_thirds_sag -->
</line>
```

The meaning of every value is that of the line file ([Line files](../user-guide/line-file.md)). Like a line file, an OHL XML line is checked (references, stringing, alignment ties) when it is read.
