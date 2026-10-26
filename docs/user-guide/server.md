# Server mode (pre-alpha)

Several people work on one project at the same time, like players on a game server: one computer **hosts** the project, the others **join** it. Every edit, from anyone, is checked by the server, applied in one order, written to the project file and sent to everyone (ADR-0005).

## Hosting

- **In the application:** File › **Host this project…** (a project file `.ohl` open): the port (UDP, default 4433) and what happens to people the project does not know (they join as viewers, as designers, or are refused). The server runs inside the application, and the application joins it like everyone else. File › **Stop hosting** ends it and opens the project file again.
- **Without the application:** `ohl serve PROJECT.ohl` (options `--bind 0.0.0.0:4433`, `--advertise HOST`, `--newcomers viewer|designer|none`, `--invite USER` repeatable, `--code SECRET`, `--keys FILE`, `--web PORT` for the [web client](#web-client), `--seconds N` to stop after a time). It prints the invitations and who joins and leaves, and runs until stopped (Ctrl+C). Every edit is already in the project file.

The project file stays on the hosting computer. The port must be reachable over UDP from the others' computers (firewall, port forwarding over the internet).

## Invitations

An invitation looks like `ohl://192.168.1.20:4433/3f2a9c1b07de/80854a…` (the address, an access code, and the fingerprint of the server's certificate; older `ols://` invitations are still read). The connection is encrypted (QUIC, TLS 1.3), and the application only talks to the server whose certificate matches the fingerprint.

| Invitation | Who joins with it |
|---|---|
| The **general** invitation (People › Invitation; `ohl serve` prints it) | Only names the project does not know. They get the newcomers' role. |
| A **personal** invitation (People › Personal invitation for NAME, while hosting; `ohl serve --invite NAME`) | That user only, with their role in the project. |

A user the project knows can only join with their own personal invitation, so nobody joins under someone else's name.

Where the codes come from (since 2026-10-26):

- The **general** access code comes from a secret made when the server starts, or from the one you give (`--code SECRET`, to keep the general invitation across restarts; links made with a code before 2026-10-26 stay valid). Everybody holding the general invitation knows that code, and a short `--code` can be guessed from it: nothing else is made from it.
- The **personal** access codes (128 bits each) come from a random key that has nothing to do with `--code`. It is made when the server starts, so the personal invitations change at every start, unless it is kept in a key file: `--keys FILE` (made with a fresh key when missing, readable by you only). With `--code` and no `--keys`, `ohl serve` keeps it in `PROJECT.ohl.server-key` beside the project and says so. Keep that file private (never in a shared folder, a repository or a mail with the project): whoever has it can make anybody's personal invitation. Personal invitations made before 2026-10-26 no longer work: hand out new ones.

## Joining

File › **Join a server…** (also on the start page): the invitation and your user name. The project is copied to this computer (with its point clouds; a progress bar shows the transfer), then opened like any project. From then on it follows everyone's edits.

## Working together

- **Edits** work as usual: each is sent to the server, which applies it or refuses it (the reason shows in the status bar). Others' edits appear between frames, and the sections they touch are solved again.
- **Undo** is per user: Undo takes back your last edit, never someone else's.
- **Roles:** viewers read; designers edit; admins also change roles (People) and reverse anyone's edits (Versions › **History…**: **Reverse** one entry, or **Back to here**, the line as it was just after an entry; both are undoable).
- **People** (View › People) lists who is connected: their role, their selection, what they are editing, and **Go to** to look where they look. Others are also shown in the plan view, as a diamond with their name.
- **Soft locks:** a structure in someone's Move window is theirs until they close it. Others cannot open it, and the server refuses their edits of it — and their **undo** or **redo** when the edit it takes back touches it. **Admins** pass the locks (their edits, merges and roll-backs go through).
- **Project settings** (the surface of the checks, the drawing settings of a project line): changed by a designer whose edits no edit right limits (on a protected project line, by an admin), sent to everyone, and written in the server's log (who changed which setting). A setting never holds a path of one computer: the server refuses such a value, and the copy of the project a joining user receives keeps only the file name (e.g. of the weather case file used last).
- **Versions** (Versions › Revisions and branches…): commits, branches, merges, reviews and edit rights go through the server too, and every replica keeps the same history. Every user works on their own project line (Versions › Project lines): edits go to it, and others see them when they work on or show that line. See [Project lines and stages](project-lines.md) and [Versions, branches and reviews](versions.md).
- Libraries and project lines also go through the server. What each person **displays** is their own: the surface used as the ground and the others shown, the LiDAR classes hidden, the share of points, the layers — remembered on their computer, never sent to the server. Importing terrain surfaces and point clouds is done on the hosting computer.

## Web client

People without OpenOHL join in a browser. Host with **Web client** ticked in File › Host this project (TCP port, default 8443), or `ohl serve PROJECT.ohl --web 8443`. The link is shown in People (**Web client**, and **Copy web link** next to a personal invitation) and printed by `ohl serve`:

    https://192.168.1.20:8443/#user=bob&code=57b8cd31f0e1

Opening it joins at once; without the part after `#`, the page asks for the name and the access code. The same codes and rules apply as for the application: a user of the project needs their personal code, others get the newcomers' role.

The web client is a **viewer**: it shows the project's **main line** and does not edit,
whatever the user's role (the server refuses edits and locks from browsers). What the page
shows:

- **3D** (WebGPU when the browser has it, else Canvas2D), **Plan** and **Profile** (along the
  main route; no terrain yet): structures, alignments, and the wires solved on the server at
  **15 °C, no wind** (one weather case for now). The line is drawn again after each change.
- **Moving around:** drag (3D: orbit; Shift or right drag: pan; plan and profile: pan), the
  wheel or **+ / −** to zoom, **W A S D** or the arrow keys to move, **F** to fit the line.
- **G: go to a structure.** Type its name (completed from the list; case does not matter, the
  start of a name is enough when only one fits) and press Enter. The view centres on it and
  selects it.
- **Click** a structure or a wire for its **properties**:
  - a structure: type, extension, position, orientation, height, the sections through it, and
    its attachments (wire point height, string length);
  - a wire span: section, span and its structures, length and chord, height difference, sag,
    largest tension, cable, bundle, circuit, phase, weather case.

  **Escape** clears the selection.
- **Display** (remembered in the browser): structures, wires, alignments, structure names,
  sags, where the others look; and the **detail**:
  - **Low:** large lines or slow computers;
  - **Medium:** the default;
  - **High:** every point.

  A wire span small on the screen is drawn with a few points or as its chord, a structure far
  away as one stroke, and what is off the screen not at all. Names and sags are written
  nearest the middle of the view first, never over each other, up to a number per detail
  level. The panel says what was drawn and how long it took. Without WebGPU, a line of
  2000 structures draws in about 10 ms at Medium; spans are sent with at most 24 points each.
- **People:** who is connected, what they selected, **Look** to look where they look; others
  are diamonds in the views.
- **Operations:** every edit as it arrives (on which project line), with its log entry.

**Security.** The page and the WebSocket are served over **HTTPS with the server's self-signed certificate**, so the browser warns the first time ("not private", "not trusted"). Before accepting it, compare the certificate's SHA-256 fingerprint (the browser shows it in the certificate details) with the one `ohl serve` prints, or with the last part of the `ohl://` invitation: if they differ, do not continue. The access code travels in the link's `#` part, which browsers do not send to the server or other sites, but it is in the browser history: hand links out like invitations. The listener serves only the client's own four files and the WebSocket; messages have the same size limits as the application's, and a browser silent for 30 s is disconnected. On a network you do not trust, prefer the application (its certificate is checked against the invitation automatically).

## Limits against misuse

Somebody holding an invitation must not be able to stop the others (security review 2026-10-26):

- A request that fails inside the server (a bug) is refused to its sender ("an internal error of the server"); the server reads the project from its file again and keeps running for everyone.
- **Joining:** at most 12 joins a minute from one address; after a **wrong access code** the address waits 1 s, then 2 s, 4 s … up to 5 minutes (a right code resets it); at most 8 connections of one address that have not joined yet (application and browser each), at most 256 application connections in all — both are the host's parameters (`ohl serve --max-per-address N --max-connections N`, or **Connections at most** and **Per address at most** in File › Host this project) and hold for this computer's own connections too: behind a reverse proxy every user comes from `127.0.0.1`, so raise the per-address limit there. A copy of the project is shared by everyone joining while nothing changed, at most 8 copies exist at once (more joins are asked to try again in a minute), and a copy not taken within a minute (plus 1 s per 256 KiB) is dropped. The general invitation adds at most 200 new users in one run of the server. The join rate and the waits after wrong codes leave the computer itself (`127.0.0.1`) out.
- **Presence and locks:** a connection's presence updates beyond 10 a second (20 at once) are dropped, its lock requests beyond 20 a second refused; the list of users is sent to everyone at most 10 times a second.
- **Messages** are read as their bytes arrive (an announced size alone allocates nothing), at most 64 MiB each.
- **Names:** a user name has no invisible characters (zero-width spaces, direction marks), and a newcomer whose name looks like a user's (`Аdmin` with a Cyrillic A beside `admin`; letter case, `0`/`o`, `1`/`l`/`i`, spaces, `-`, `_` and `.` ignored; Cyrillic and Greek look-alikes, full-width letters) is refused.
- **Reviews:** approved or rejected only by designers or admins, never by the review's author ([Versions, branches and reviews](versions.md#reviews)).
- The copies of the project for joining users are written in a folder of the temporary folder that only the server's user may open.

## Limits for now

- Edits wait for the server's answer (milliseconds on a network; at most 15 s) before they show. Edits are not shown before the server confirms them.
- Joining copies the whole project. Only the chunks a user views will be sent, later.
- The application speaks QUIC only (UDP). Browsers use the web client's WebSocket (TCP); the application cannot use it yet, for networks that block UDP.
- The web client: a viewer of the main line; one weather case, no terrain, point clouds or backgrounds.
- A user's name and the access code are the only identity. Sign-in through an organisation (OIDC / SSO) is to come.
- If the connection drops, the project turns read-only. Join again to continue.
