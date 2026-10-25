# 1. A new project and its ground

**Goal:** a new project made from the starter template, with the
invented ground as its terrain surface.

## Steps

1. Start OHL-Design. On the start page click **New project from the
   starter template…** (or, with a project open, **File › New project…**
   and tick **Starter template (for learning)**).
2. In the **New project** window keep the line name *My first line* (or
   type your own) and **Local coordinates (metres, no geographic
   reference)**. Units are SI.
3. Click **Choose where to save it and create…** and choose a new, empty
   folder for the project file (`My first line.ohl`). The project opens.
4. The window **New project: first steps** asks for the route. Click
   **Later**: the ground comes first, so that the alignment points get
   their heights from it.
5. **Terrain › Terrain surfaces (TIN)…**. In the window, under the
   import, click **Import…** and choose `starter-survey.csv` — the
   starter put it beside your project file.
6. Close the window when the surface is listed.

## What you should see

The Terrain surfaces window lists *starter-survey*, and it is the
ground. The 3D view shows a gently rolling piece of land about 1.4 km by
800 m (E from −100 to 1320 m, N from −160 to 620 m), heights between
about 494 and 512 m. The Next steps page has *The ground* done, and *Wire types*,
*Structure types*, *Weather cases* and *Load cases* too: the starter
brought them.

**What just happened:** the project file keeps the line (empty so far)
and copies of the starter's libraries; the structure templates and the
survey went beside it, into your folder. The survey points were
triangulated into a terrain surface kept in the project, and the
surface became the ground every check and drawing measures from. See
[Starting from nothing](../user-guide/desktop-app.md#starting-from-nothing),
[Terrain surfaces in the project](../user-guide/terrain.md#terrain-surfaces-in-the-project)
and [Project files](../user-guide/project.md).

Next: [2. The route and the alignment](02-route-and-alignment.md).
