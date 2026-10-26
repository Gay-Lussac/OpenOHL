# 0. Before you start

**Goal:** know what you need and how the views and the tools work.

## What you need

- OHL-Design, built from the OpenOHL source tree: on macOS double-click
  **Start OHL-Design.command**; elsewhere
  `cargo run --release -p ohl-app` (see [Getting
  started](../user-guide/getting-started.md)).
- The starter template, `examples/starter/` of the source tree. It comes
  with the program; you do not open it yourself.
- A mouse with a right button (or a trackpad set up for right-clicks:
  a right-click opens an object's menu).

## The starter template

The starter template is a set of example libraries with invented or
typical values, clearly marked:

- a **wire library** with an ACSR conductor (`acsr-drake-26-7`);
- two **structure types** of detail level 1, built from the structure
  template `delta-1c`: `starter-suspension` (one string per phase, three
  phases in a delta and an earth wire at the peak) and `starter-tension`
  (a tension structure: two tension strings at each place, one for the
  span behind, one for the span ahead);
- insulator assemblies, five **weather cases** (everyday, max
  temperature, glaze ice, high wind, minimum temperature), three **load
  cases** and a **clearance rule file**;
- `starter-survey.csv`, survey points of an invented piece of ground.

It has **no route, alignment, structure or wire**: those you make in
this tutorial. The folder's README lists every file.

## The window

- **Menus** at the top: File, Edit, View, Terrain, Structures, Sections,
  Criteria, Checks, Tools, Versions, Plugins, Help. Right of them the
  **quick bar** (Undo, Redo, Save all, …).
- **Pages** like browser tabs: **3D**, **Plan**, **Profile**,
  **Properties** (the selected object), **Next steps**, and others from
  the View menu. Drag a tab to move it; closed pages come back from View.
- The **status bar** at the bottom says what a tool expects next. Read
  it whenever you are unsure.

## Tools: hold one, click to use it

The bottom of the 3D view holds the **hotbar**, nine slots of tools.
Choosing a slot (keys **1–9** or a click) only *holds* the tool. A
**click** on an object, in the 3D view, the plan or the profile, *uses*
it; with **Select** held a click selects. **Escape** holds Select again.
A **right-click** on an object selects it and opens its **menu**:
Properties, Edit…, Delete…, Zoom to and the tools that apply to it.
Nothing moves on a click or a drag with Select: a structure moves with
the **Move structure** tool.

A new hotbar holds Select, Move structure, Delete, Go to structure,
Edit, Add structure, Add string, String wire and New alignment. **E**
opens the **inventory** with every tool; the **Tools** menu lists them
too (Edit › Tools). Holding a tool that is not in the hotbar puts it
there.

## Next steps

The **Next steps** page (View › Next steps) lists what the line still
needs, in the order of the work, and each step is a link to where it is
done: a tool held, a window opened. The next step is in bold. Whenever
you lose your way in this tutorial, look there.

**What just happened:** nothing yet — but every chapter ends with a
paragraph like this one, saying what the program did and where to read
more. The whole window is described in [OHL-Design, the line
application](../user-guide/desktop-app.md); the controls of each view in
[Controls](../user-guide/desktop-app.md#controls).

Next: [1. A new project and its ground](01-new-project.md).
