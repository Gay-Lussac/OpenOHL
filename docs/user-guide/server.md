# Server mode (alpha)

Several people work on one project at the same time, like players on a game server: one computer **hosts** the project, the others **join** it. Every edit, from anyone, is validated by the server, applied in one order, written to the project file and sent to everyone (ADR-0005).

## Hosting

- **In the application:** File › **Host this project…** (a project file `.ols` open): the port (UDP, default 4433) and what happens to people the project does not know (they join as viewers, as designers, or are refused). The server runs inside the application, and the application joins it like everyone else. File › **Stop hosting** ends it and opens the project file again.
- **Without the application:** `ols serve PROJECT.ols` (options `--bind 0.0.0.0:4433`, `--advertise HOST`, `--newcomers viewer|designer|none`, `--invite USER` repeatable, `--code SECRET`). It prints the invitations and who joins and leaves, and runs until stopped (Ctrl+C). Every edit is already in the project file.

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
- Libraries and project lines also go through the server. What each person **displays** is their own: the surface used as the ground and the others shown, the LiDAR classes hidden, the share of points, the layers — remembered on their computer, never sent to the server. Importing terrain surfaces and point clouds is done on the hosting computer.

## Limits of the alpha

- Edits wait for the server's answer (milliseconds on a network; at most 15 s) before they show. Edits are not shown before the server confirms them.
- Joining copies the whole project. Only the chunks a user views will be sent, later.
- QUIC only. A WebSocket fallback, for networks that block UDP, is to come.
- A user's name and the access code are the only identity. Sign-in through an organisation (OIDC / SSO) is to come.
- If the connection drops, the project turns read-only. Join again to continue.
