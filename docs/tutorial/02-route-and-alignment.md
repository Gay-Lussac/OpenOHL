# 2. The route and the alignment

<img src="../images/tutorial/tutorial-alignment-plan.png" alt="You will end with this: six alignment points in the plan, straight east, then turning left at the fourth" width="400">

**Goal:** a route, and an alignment of six points with an angle at the
fourth.

## Steps

1. On the **Next steps** page click **Next: Name the route…**.
2. Keep the name *route 1* and click **Create the route**. The **New
   alignment** tool is now held.

   ![The first steps window: route 1, Create the
   route](../images/tutorial/tutorial-name-the-route.png)

3. Click the ground in the **3D** view, anywhere. The window **Point
   coordinates** opens.
4. Type **E** and **N** of point 1 from the table, keep the height and
   click **Add the point**.

   | Point | E (m) | N (m) |
   |---|---|---|
   | 1 | 0 | 0 |
   | 2 | 200 | 0 |
   | 3 | 400 | 0 |
   | 4 | 600 | 0 |
   | 5 | 800 | 100 |
   | 6 | 1000 | 200 |

   ![The Point coordinates window: E 200, N 0, the ground's
   height](../images/tutorial/tutorial-point-coordinates.png)

5. Repeat steps 3 and 4 for points 2 to 6. A yellow line joins the
   points.

   ![Mid-way: three points, joined by a yellow
   line](../images/tutorial/tutorial-alignment-clicking.png)

6. Press **Enter**.
7. Click **Make the alignment** (*route 1* is chosen).

   ![The window New alignment: its route](../images/tutorial/tutorial-alignment-route.png)

## What you should see

On the **Plan** page (the tab next to 3D; scroll to zoom): points P1 to
P6, straight east for 600 m, then turning left by about 27° at P4.

![The Plan page with the six alignment points](../images/tutorial/tutorial-alignment-plan.png)

> **If a point is wrong:** **Backspace** takes the last point back,
> **Escape** gives the alignment up. Later, correct E and N in **Line ›
> Terrain › Alignment points table**.
>
> **If no window opens at a click:** the click took an existing point.
> Press **Backspace** and click beside it.

<details>
<summary>Why it works this way</summary>

A **route** is the path of the line; the structures, wires and checks
belong to it. Its **alignments** are the polylines the structures stand
on, through their **alignment points**. Each point of the table lies on
a survey point, so its height is the surveyed one. You may click in the
plan instead; the 3D view shows the ground and the points while you
click.

</details>

Read more: [Routes and alignments](../user-guide/desktop-app.md#routes-and-alignments).

Next: [3. The structures](03-structures.md).
