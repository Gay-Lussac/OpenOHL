# Server mode (alpha)

Several people work on one project at the same time, like players on a game server: one computer **hosts** the project, the others **join** it. Every edit, from anyone, is validated by the server, applied in one order, written to the project file and sent to everyone (ADR-0005).

## Hosting

- **In the application:** File › **Host this project…** (a project file `.ols` open): the port (UDP, default 4433) and what happens to people the project does not know (they join as viewers, as designers, or are refused). The server runs inside the application, and the application joins it like everyone else. File › **Stop hosting** ends it and opens the project file again.
- **Without the application:** `ols serve PROJECT.ols` (options `--bind 0.0.0.0:4433`, `--advertise HOST`, `--newcomers viewer|designer|none`, `--invite USER` repeatable, `--code SECRET`, `--web PORT` for the [web client](#web-client), `--seconds N` to stop after a time). It prints the invitations and who joins and leaves, and runs until stopped (Ctrl+C). Every edit is already in the project file.

The project file stays on the hosting computer. The port must be reachable over UDP from the others' computers (firewall, port forwarding over the internet).

## Invitations

An invitation looks like `ols://192.168.1.20:4433/3f2a9c1b07de/80854a…` (the address, an access code, and the fingerprint of the server's certificate). The connection is encrypted (QUIC, TLS 1.3), and the application only talks to the server whose certificate matches the fingerprint.

| Invitation | Who joins with it |
|---|---|
| The **general** invitation (People › Invitation; `ols serve` prints it) | Only names the project does not know. They get the newcomers' role. |
| A **personal** invitation (People › Personal invitation for NAME, while hosting; `ols serve --invite NAME`) | That user only, with their role in the project. |

A user the project knows can only join with their own personal invitation, so nobody joins under someone else's name. Access codes come from a secret made when the server starts: invitations change every time it starts, unless you give the secret (`--code`).

## Joining

File › **Join a server…** (also on the start page): the invitation and your user name. The project is copied to this computer (with its point clouds; a progress bar shows the transfer), then opened like any project. From then on it follows everyone's edits.

## Working together

- **Edits** work as usual: each is sent to the server, which applies it or refuses it (the reason shows in the status bar). Others' edits appear between frames, and the sections they touch are solved again.
- **Undo** is per user: Undo takes back your last edit, never someone else's.
- **Roles:** viewers read; designers edit; admins also change roles (People) and reverse anyone's edits (Edit › **History…**: **Reverse** one entry, or **Back to here**, the line as it was just after an entry; both are undoable).
- **People** (View › People) lists who is connected: their role, their selection, what they are editing, and **Go to** to look where they look. Others are also shown in the plan view, as a diamond with their name.
- **Soft locks:** a structure in someone's Move window is theirs until they close it. Others cannot open it, and the server refuses their edits of it.
- **Versions** (Edit › Versions…): commits, branches, merges, reviews and edit rights go through the server too, and every replica keeps the same history. There is one working state for everyone: a checkout or merge changes it for all. See [Versions, branches and reviews](versions.md).
- Libraries and project lines also go through the server. What each person **displays** is their own: the surface used as the ground and the others shown, the LiDAR classes hidden, the share of points, the layers — remembered on their computer, never sent to the server. Importing terrain surfaces and point clouds is done on the hosting computer.

## Web client

People without OLS CAD join in a browser. Host with **Web client** ticked in File › Host this project (TCP port, default 8443), or `ols serve PROJECT.ols --web 8443`. The link is shown in People (**Web client**, and **Copy web link** next to a personal invitation) and printed by `ols serve`:

    https://192.168.1.20:8443/#user=bob&code=57b8cd31f0e1

Opening it joins at once; without the part after `#`, the page asks for the name and the access code. The same codes and rules apply as for the application: a user of the project needs their personal code, others get the newcomers' role.

What the page shows:

- **3D** (WebGPU when the browser has it, else a simpler Canvas2D drawing), **Plan** and **Profile** (along the main route; no terrain yet): structures, alignments, and the wires solved on the server at **15 °C, no wind** (one weather case for now). Drag, Shift+drag and the wheel move the view.
- **Selection:** click a structure (position, type, attachments) or a wire (section, span, cable, sag, largest tension).
- **People:** who is connected, what they selected, **Look** to look where they look; others are diamonds in the views. Your selection and view are shown to them in turn.
- **Operations:** every edit as it arrives, with its log entry. The line is drawn again after each change (solved once on the server for every browser).
- **Designers** can move a selected structure (x, y, z). Everything else is read-only in the browser for now.

**Security.** The page and the WebSocket are served over **HTTPS with the server's self-signed certificate**, so the browser warns the first time ("not private", "not trusted"). Before accepting it, compare the certificate's SHA-256 fingerprint (the browser shows it in the certificate details) with the one `ols serve` prints, or with the last part of the `ols://` invitation: if they differ, do not continue. The access code travels in the link's `#` part, which browsers do not send to the server or other sites, but it is in the browser history: hand links out like invitations. The listener serves only the client's own four files and the WebSocket; messages have the same size limits as the application's, and a browser silent for 30 s is disconnected. On a network you do not trust, prefer the application (its certificate is checked against the invitation automatically).

## Limits of the alpha

- Edits wait for the server's answer (milliseconds on a network; at most 15 s) before they show. Edits are not shown before the server confirms them.
- Joining copies the whole project. Only the chunks a user views will be sent, later.
- The application speaks QUIC only (UDP). Browsers use the web client's WebSocket (TCP); the application cannot use it yet, for networks that block UDP.
- The web client: one weather case, no terrain or point clouds, no undo, moving structures only.
- A user's name and the access code are the only identity. Sign-in through an organisation (OIDC / SSO) is to come.
- If the connection drops, the project turns read-only. Join again to continue.
