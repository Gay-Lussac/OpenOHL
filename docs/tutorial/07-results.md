# 7. Results

**Goal:** read the line's results — in the plan, the profile and 3D, and
the sags and tensions in the properties and the tables.

## Steps

1. **View › Fit views**. The **Plan** shows the alignment, the six
   structures and the wires from above; the **3D** view the line on its
   ground (drag to turn, scroll to zoom).
2. The **Profile**, under the 3D view: the ground along the alignment
   (with the survey points beside it), the structures by station and the
   wires as solved.
3. **Line › Sections › Display options**: tick **Sag** for the Profile (the
   menu stays open). Each span's sag is written at mid-span.
4. Left-click a span of S1 in any view. **Properties** gives its wire
   type, circuit and phase, and per weather case its horizontal and
   maximum tension, its sag and its lowest point.
5. **Line › Sections › Sections table**: one row per section, with **H kN** and
   **max sag m** in the weather cases shown.
6. Hold **Edit** and click the same span: its section's window.
   Open **Sagging** for the crew's table (sags per span and temperature).
   Click **Cancel**.

## What you should see

Spans of 200 m and 224 m, sags of several metres, more at the
maximum temperature and with glaze ice than in the everyday case. The
two sections of a phase have the same stringing tension, so similar
sags in similar spans.

![The tutorial line: in 3D on its ground, in the profile below, a span's
results in every weather case on the right](../images/tutorial/tutorial-line-3d.png)

**What just happened:** nothing was computed now — every result was
solved when the line changed, in each weather case shown (the others
when first shown). Every view, table and check reads the same solved
line. See [Views](../user-guide/desktop-app.md#views),
[Tables](../user-guide/desktop-app.md#tables), [Sagging
outputs](../user-guide/sagging.md) and
[Sag-tension](../theory/sagtension.md).

Next: [8. Checks](08-checks.md).
