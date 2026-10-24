# Project lines and stages (the Versions menu)

A project (`.ohl`) can hold several designs of the line side by side:
**project lines**. The first is the **main line**. Others are made from it,
typically one per **stage** of the project (*stage 1*, *stage 2*…) or per
alternative (*route B*). They are often shown next to the main line, for
example the line *as built* next to the line *as planned*.

Every project line is **edited on its own**: moving a structure on *stage 1*
changes *stage 1* only.

## The ideas in short

- **You work on one line at a time.** Your edits go to it, and Undo (⌘Z)
  takes back your last edit *on that line*. Choosing another line changes
  nothing; it only changes what you edit. On a server, every user chooses
  their own line, independently of the others.
- **Other lines can be shown next to it**, each in its colour, in the 3D
  view (layer *Other project lines*) and the plan. By default only **what
  differs** from the line you work on is drawn — its sections, structures
  and jumpers that are changed or added (a section also when one of its
  structures is); tick **whole lines** in the Versions menu to draw them
  whole. They are drawn as the line you work on is: structures, strings at
  their solved clamps, bundles and jumpers, in the same weather cases. A
  line shown is solved only in the weather cases displayed, in the
  background, from the solutions of the line you work on with only the
  sections that differ solved again (2026-10-23; before, every weather case
  of the project was solved at once, which held the window for a long
  time). Each line has its own library files (wires, structure types,
  insulators, rules…), tracked with it like its objects; a line shown is
  read and solved with its own where they differ from yours — the status
  bar says which. The lines on the ground and the sheets take it whole. When
  someone else edits a line you show, it is redrawn.
- **A new line is a copy.** It starts as the line it is made from is at that
  moment, including changes not yet committed. From then on the two are
  independent: a correction made later on the main line does **not** appear
  in the stage by itself. The project file keeps a new line as the revision
  it starts from and only what the line changes, so starting a line is
  quick and takes next to no room, however long the line.
- **Bringing in changes** updates a line with what changed on another since
  they parted. Changes to different objects combine by themselves. Where
  both lines changed the same object, you choose which version to keep.
- **Back into the main line through a review.** When a stage is ready, it
  is submitted for review. A reviewer or an admin approves it, then merges
  it into the main line.

What all lines share: the terrain surfaces, the point clouds, the
backgrounds and the project settings. What each line has of its own: the
line itself (structures, sections, jumpers, routes, line settings), its
libraries (wire types, structures, insulators, weather cases…) and its history
of edits. A new line starts with the libraries of the line it is made from;
a library changed on one line leaves the others as they are.

## The Versions menu

The menu stays open while you tick lines to show.

| Item | What it does |
|---|---|
| **Working on: *name*** | The line your edits go to. |
| **Project lines ›** **Project lines…** | The window listing every line: choose the one you work on and those shown; rename, delete (below). |
| **Project lines ›** **Working tree…** | The lines through time (below). |
| **Description of *name*…** | What the line you work on is for (below). |
| **New line from *name*…** | A copy of the line you work on, under a new name (e.g. *stage 1*). You keep working on your line until you choose the new one. |
| **Bring in changes from another line…** | Brings what changed on another line (by default the main line) into yours. |
| **Submit this line for review into the main line…** | When a stage is ready to go into the main line (greyed out on the main line itself). |
| ☐ *line* … | Tick to show a line next to yours, in its colour; untick to hide it. |
| ☐ **whole lines (else only what differs)** | Draw the lines shown whole; by default only what differs from the line you work on. |
| **History…** | The operation log: who did what; admins reverse entries. |
| **Revisions and branches…** | Revisions, branches, tags, diff, merge, reviews and edit rights (see [Versions, branches and reviews](versions.md#in-the-desktop-app)). |

## The Project lines window

One row per line:

| Column | |
|---|---|
| **work on** | The line you work on (●). Click another to work on it: it is read and solved again; tick the line you left under **show** to keep seeing it. |
| **show** | Shown next to the line you work on. The line you work on is always shown. |
| **line** | Its name, in its colour when shown. Hover for who made it, when, from which line, and how many objects it holds. |
| **made by** | Who made it (*main line* for the main line). |
| **objects** | Structures, sections, jumpers, routes and settings it holds. |
| **Description…** | Its description: read it (hover shows its start), edit it if you may. |
| **Rename…**, **Delete…** | Greyed out when you may not (hover says why). |

Below the list: **New line from *name*…**, **Bring in changes…** and
**Submit for review…**, as in the menu.

- **Rename** a line: its history, reviews, and whoever works on it follow the
  new name.
- **Delete** a line: it goes with its uncommitted changes. Its committed
  revisions stay in the file: *Versions › Revisions and branches › New line from it* brings
  it back. Its open reviews are closed. Whoever was working on it is moved
  to another line. The line you work on and the project's only line cannot
  be deleted. Deleting is not undone with ⌘Z.

## The working tree

**Versions › Project lines › Working tree…** (also in the Project lines
window) draws the history of the project lines. The oldest is on the left
and time runs to the right: one column per revision, with the date under
the first of each day.

- **One lane per line**, the main line on top, each in its colour; *(you)*
  marks the line you work on. A line's lane starts where it was made from
  another, joined to it by a curve. Hover a line's name for who made it,
  when, and from what.
- **A dot per revision.** The larger dot is the line's latest revision; a
  ringed dot is a **merge** (changes brought in, or a review merged), joined
  to the line the changes came from. Tags are written above their revision.
- **An open dot** after a line's last revision means it has work not
  committed yet.
- **Grey, *lines deleted*:** revisions of lines that no longer exist.
- **The description** of a line is shown below the tree in a box of fixed
  height that scrolls. It is the line you work on at first; click a line's
  name, a revision, or an open dot to show that line's. **Edit…** opens it
  (editable by its creator or an admin).
- **Click a revision** for its message, author, time and line.
  - **New line from it…** starts a line from that revision: *copy of*
    names it in words (its line, when, its message, e.g. *main at
    06.10.2026 14:32 — T2 corrected*); the list also offers each line as it
    is now.
- **A line with nothing committed of its own yet** has a small ring where it
  started (click: the revision it started from).
- **Click an open dot** to see a line's uncommitted work. On the line you
  work on, type a message and **Commit** it there.
  - **Roll back** (admins only): choose the line (by default the
    revision's own) and confirm. The line's state becomes the revision's.
    The line's uncommitted work is recorded first, and the rollback is
    recorded as a new revision, so nothing is lost and a rollback can itself
    be rolled back. Others who work on or show that line see it change.
    Designers see *Rolling a line back: admins only*.

A line whose changes were merged into another by a *fast-forward* (the other
line had not changed meanwhile) shares its revisions with it: they are drawn
on the upper lane.

### Descriptions

Every line has a **description**: free text such as its scope, the
assumptions behind it, or its status. It may be long (up to 20 000
characters). It is edited in a box of fixed height that scrolls, from the
Project lines window (**Description…** on its row) or from the Versions menu
(**Description of *name*…** for the line you work on). The working tree
shows it below the tree (see there). Who may edit it is who may rename the
line: its creator or an admin; the main line, an admin. The others read it.
It is kept in the project and follows a rename.

### Who may do what

| | Viewer | Designer | Admin |
|---|---|---|---|
| Choose the line to work on, show lines | yes | yes | yes |
| Edit the line worked on | — | yes | yes |
| Make a new line | — | yes (it is theirs) | yes |
| Rename, delete or describe a line | — | their own lines | every line |
| Rename, delete or describe the **main line** | — | — | yes |
| Bring in changes | — | yes | yes |
| Submit a line for review | — | yes | yes |
| Approve and merge a review | — | if named reviewer | yes |
| Roll a line back (working tree) | — | — | yes |

Admins can also **protect** a line (Versions › Revisions and branches): it then changes only
through admins or approved reviews.

## A stage, step by step

1. On the main line, **Versions › New line from main…**, name it *stage 1*,
   **Create**.
2. **Versions › Project lines…**, click *stage 1* under **work on**. Edit it:
   replace structures, restring sections… The main line does not change. Tick
   *main* under **show** to see it next to your stage.
3. Meanwhile, a colleague corrects structure T3 on the main line. Your stage does
   not change. When you want the correction, choose **Versions › Bring in
   changes from another line…**, *changes of: main*.
   - If you did not change T3 yourself, it simply comes in: the window says
     **No conflict**. Press **Bring in**.
   - If you changed T3 too, the window lists *structure T3*. Choose **keep
     this line's** or **take main's** (or **All: …** for every object listed),
     then **Bring in**.

   It is refused, changing nothing, while an object is not settled.
4. When the stage is ready, choose **Versions › Submit this line for review
   into the main line…**, with a note. Uncommitted work is committed first.
5. A reviewer or an admin opens **Versions › Revisions and branches › Reviews**, checks the
   changes (**Changes**, with the engineering impact), **approves**, and
   **merges**. Only then does the main line change. If the stage changes
   after approval, it must be submitted again.

Starting a line, bringing in changes and submitting a review record the
lines' state as **revisions** (Versions › Revisions and branches shows them): they are the
common ground the next bring-in or merge compares against. Under the hood, project lines are the
branches of the project's version history; see
[Versions, branches and reviews](versions.md).

## On a server

Each user works on their own line, and their requests carry it.
Edits of a line reach everyone, but they only change what users who work on
that line, or show it, see. When someone renames the line you work on, you
follow it. When someone deletes it, you are moved to another line and told
so. Soft locks are per line: holding T3 on *stage 1* does not block T3 on
the main line. The web client shows the main line.

## On the drawings

Other project lines can be drawn on the plan and profile sheets with the
line worked on, each in a **role** (existing, project, to be removed,
decommissioned …) the sheet template styles — e.g. the existing line
thin, the new one thick, what is removed in its own colour: File › Export
› Drawings… › **Versions of the line, roles**, or `ohl drawing PROJECT.ohl
--version "as built=existing"`. See [Sheet
templates](sheet-templates.md#versions-of-the-line-kinds-and-roles).

## Your choice is remembered

The line you work on and the lines you show are remembered on your computer
for each project, like the other display choices. The project opens on them
next time.

## Projects made before

Projects saved by earlier versions kept **frozen** project lines (*Create
line* saved a copy that could not be edited). They are converted when the
project is opened: each becomes an editable line of the same name and
creator. Their working state is the one saved. They have no revision in
common with the main line, so bringing changes between them compares
whole objects.

A project opened by this version is marked as project schema 6 (its
lines can be kept as differences) and can then no longer be opened by
earlier versions: they say the project is newer than they support. Lines
of older projects stay as they are when opened; only lines started from
then on are kept as differences.

## On the command line

```bash
ohl project branch project.ohl "stage 1" --user ben          # a new line from the one the file opens on
ohl project checkout project.ohl "stage 1" --user ben        # the commands act on it from now on
ohl project apply project.ohl --user ben '{"op": "move_structure", "id": "T6", "position": [1500, 400, 505]}'
ohl project checkout project.ohl main --user ana             # back to the main line
ohl project branch project.ohl                               # list the lines (branches) and tags
ohl project branch project.ohl "stage 1" --delete --confirm --user ben
ohl project history project.ohl                              # the revisions with their lines and tags
```

`checkout` also sets the line the file opens on; nothing else changes.
