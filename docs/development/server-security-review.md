# Security review of the network server (phase 13, 2026-11-20)

*In active development, not approved yet* ([ADR-0013](../adr/0013-country-server.md),
[decision log](decisions.md#2026-11-20-country-server-phase-13)). The
country server's server (`ohl network serve`, `crates/ohl-server`
`network*.rs`, `server.rs`, `web.rs`, `stream.rs`) and its database
(`crates/ohl-store-pg`) reviewed before real data goes in, as the ADR
asks. It follows the review of 2026-10-26 (the file server: invitations,
a failing job, locks, settings, names — `security_tests.rs`), whose
fixes hold for both servers and are not repeated here.

**Who the attacker is.** (a) Anyone who can reach the server's port
(QUIC over UDP, and the web client's TCP port if served) without an
invitation; (b) a user of the network — someone with a personal
invitation or a sign-in mapped by an admin — trying to see what their
rights hide, or to stop the others; (c) someone reading what the
server leaves behind (logs, temporary files, backups). The database's
administrators and whoever holds its credentials are trusted: they can
read every row anyway.

**Severity**: high — one user (or nobody) can stop everyone or read what
is hidden; medium — a user can slow everyone markedly or hold memory;
low — a weakness needing another one, or a small effect. Each finding:
the fix and its test (`crates/ohl-server/src/hardening_tests.rs` unless
said), or "accepted" with the reason.

## Findings

| # | Finding | Severity | Fix | Test |
|---|---|---|---|---|
| S1 | **Long work on the authority.** A check on a working set (a whole route's clearances: 9.6 s on the synthetic country), an export or a bundle brought back ran on the authority thread as soon as asked: every edit of every user waited behind it, and a user per connection could ask again at once (four users: about 40 s without an edit) | High | Checks, exports and bundles **wait in a queue** and run only when no other job waits — every command, interest and join already there goes first, and one arriving while one runs goes before the next; **one per user** waiting, **at most `--max-heavy-queue`** (4) in all, more refused with "the server is busy … try again in a minute". Their parameters are checked before they wait (S2). **Phase 14**: with many users editing at once the queue of commands is never empty, so long work that may start and has waited 2 s starts anyway between two jobs (at most one such start every 2 s): on main's code forty editors kept every check and export waiting for as long as they edited | `long_work_waits_behind_the_commands`, `overdue_long_work_starts_between_jobs` (the authority's loop: commands first, a command arriving during one long work before the next), `long_work_queue_limits`, `tiles_lists_checks_and_queue_bounded` (a full queue refuses) |
| S1b | One long work, once started, still holds the authority for its length | Medium | **Fixed for checks and exports** (owner, 2026-11-20: phase 10's question 1, phase 12's question 6): a check runs on the checks' worker from a snapshot (phase 13), an offline export is decided on the authority and written on the exports' thread (phase 14) — the authority holds only the snapshot or the decision (a few hundred milliseconds on a large working set). A bundle brought back still runs on the authority (one command on a new branch, seconds at most) | `checks_on_the_worker_as_on_the_authority`, `exports_on_their_thread_as_on_the_authority` |
| S2 | **A check's reach from the client.** The field's distance (`1e9` m: the whole country pulled in), the clearance rules' distances (a rule of 50 km), the structures of a check of loads (any number), the rules' text (64 MB) were taken as sent | Medium | The field distance more than 0 and at most 2 000 m, the current and limit finite; the rules' text at most 256 KB; at most 10 000 structures — refused before the check waits. A check reads at most **10 km around its working set** (a plugin's largest declared reach): an area asked beyond it reads nothing more and the check is refused at once after, saying so (the server's neighbourhood, no engine change) | `tiles_lists_checks_and_queue_bounded` (each refusal; rules of 50 km: "more than 10000 m"); the property test sends both every round |
| S3 | **Tiles made per second.** A client could ask 256 new tiles four times a second (the interest's pace): 1 024 tile packages a second for the packager threads, whose queue had no bound; a tile dropped from the interest was still made | Medium | Each client **64 new tiles a second (256 at once)** (`--tiles-per-second`), tiles kept already free; the server makes **at most 1 024 tiles at once** for everyone; tiles over either are not made then, a notice says to ask again. A tile already queued is made once for all who wait (as before). Accepted: a tile dropped meanwhile is still made (its cost is bounded by the budgets) | `tiles_lists_checks_and_queue_bounded` (a budget of 3: at most 3 tiles, the notice) |
| S4 | **Lists in messages** bounded only by the message's size: an interest's routes, a working set's picks, the working sets one user saves | Low | At most 4 096 routes a client follows, 256 picks a working set (joins, picks, checks, exports, saves), 200 saved working sets a user. Accepted: a replica's `hold` list (one hash per object it holds) is bounded by the message size; the server compares it in one pass | `tiles_lists_checks_and_queue_bounded` (5 000 routes, 300 picks) |
| S5 | **Message size.** Every joined connection may send messages of 64 MB (a bundle needs that); 256 connections could make the server read 16 GB at once | Medium | Messages over 1 MB take their size from **one budget of 256 MB** for the whole server before they are read (a message waits for its share 30 s at most, then its connection is closed); web clients, viewers who send no bundles, are limited to **8 MB** a message after their join (16 KB before, as before). Messages are still read as they arrive, never allocated from the length announced (2026-10-26, M4) | `large_messages_share_a_budget` |
| S6 | **IPv6 per address.** The limits per address (connections not joined, joins a minute, the wait after wrong codes) counted each IPv6 address apart: one host holds a whole /64 | Low | IPv6 addresses count by their **/64**; an IPv4 address mapped into IPv6 as the IPv4 address | `ipv6_counted_by_its_64` |
| S7 | **Personal invitations never expire** and could be withdrawn only by disabling the user or replacing the server's key file (every user's invitation then) | Medium | **Reissue** (`ohl network rights reissue-invitation NAME`, network admins, audited; schema 7: each user's generation): every older invitation of that user refused, the user let go if connected. **An end**: `serve --invite-days N` gives invitations valid to the end of a day (UTC). Both are in the code (`G.UNTIL.MAC`, the MAC over the name, generation and end: neither can be changed without the key); the invitations given before stay valid until reissued. A refusal of a valid but withdrawn or expired code says so (in the audit); a wrong code says only "wrong access code" | `personal_codes_reissued_and_expiring`, `invitations_reissued_and_expiring_on_a_network` |
| S8 | **Replay and brute force of access codes** | — | Accepted: a code is a 128-bit secret sent only inside TLS 1.3 (QUIC; the server pinned by its fingerprint, HTTPS for the web client), so a replay needs the code itself; guessing is bounded by the waits after wrong codes (1 s doubling to 300 s per address, by /64 since S6) and 12 joins a minute per address. An OIDC ID token is a bearer token too: taken only fresh (issued within an hour, not expired, a minute of leeway) and only inside TLS | `security_tests.rs` (2026-10-26), S6, `oidc::tests` |
| S9 | **Rights on the messages of phases 10–12**: `check`, `export_working_set`, `import_bundle`, `create_working_set_branch` | — | Reviewed, no finding: a check needs see on its working set and pulls in hidden neighbours only counted (never named nor placed); an export needs see on the working set's routes, edit on something of each and export on each of its objects, and leaves out what its user may not see; a bundle is brought back by its exporter only, on a new branch, decided by the user's rights as they are now; a branch over a working set needs edit on its routes. Every refusal is sanitised (no hidden id). The property test now also signs in with an OIDC token every other round and sends refused checks | `property_no_message_holds_a_hidden_id_or_place` (extended), the phase tests |
| S10 | **SQL built from user input** | — | None found. Every statement with a user's value is prepared with parameters. Statements built at run time take only: savepoint names (`step_name`: letters, digits, `_`), table names from the catalogue quoted by PostgreSQL (`quote_ident`: the backups' hashes), constants (`REQUEST_SELECT`), a project file's own table names (constants, the import). The connection string is parsed into its settings, never put into SQL | reviewed; `connect::tests` |
| S11 | **Paths from clients.** The temporary copies of a model, a working set's replica and an offline export were named with the model's code (checked against the network's models before, but a path built of a client's words, and a path that existed already was named in the refusal) | Low | Every temporary file is named by the server's own words and a random part (`copy-…`, `ws-…`, `offline-…`), in its private folder (0700). Bundles hold no path (revisions, trees and blobs by hash; library files refused offline); a library file's name is checked before any is written (`safe_file_name`). A backup's manifest naming another dump than `network.dump` is refused. The command line's `--out` files are the admin's own and must not exist | `backup_and_the_restore_drill` (a manifest naming `../../etc/passwd`), reviewed |
| S12 | **Logs and secrets.** The server's options printed with `{:?}` would show the database's connection string (with its password); a password on the command line is seen by every user of the computer (`ps`) | Low | The options' and the connection's `Debug` leave the connection string and password out; the password may come from `OHL_DATABASE_PASSWORD_FILE` (a Compose secret); the PostgreSQL tools get it and the TLS settings in their environment, never on their command line; ID tokens are never logged nor stored (the audit names the issuer and subject); the invitations the server prints go to its own terminal (by design). Accepted: the server's log (stderr) is the operators' — it names users, models and object ids (a refused command's reason), never a code, token or password | `connect::tests::the_password_never_shown_nor_given_on_a_command_line` |
| S13 | **The database on another host without TLS** | Medium | Connections to a host that is not this computer need TLS unless `sslmode=disable` is said (`require`, `verify-ca`, `verify-full` with the owner's CA in `sslrootcert`; rustls); refused otherwise, saying how | `connect::tests`, `tls_to_the_database` (`ohl-store-pg`) |
| S14 | **A schema migration without a backup** | Medium | Refused unless a backup newer than the network's `backup_max_age_hours` (24) is recorded, or `ohl network migrate --i-have-a-backup` (audited) | `migrations_wait_for_a_recent_backup` (`ohl-store-pg`) |
| S15 | **The server's role in the database**: the examples used the cluster's superuser | Low | The Compose files make a role `ohl` without superuser, owner of the network's database only, PostGIS made once by the superuser; a restore by that role skips PostGIS's comment (`--no-comments`). Accepted: the server's role owns its tables (it makes them by migrating) — a role per task (migrations, the server, backups) is a later step | `compose_files_hold_together` (`ohl-cli`) |

## What was checked and holds

- **A slow client**: the server never waits for one. Its own queue (the
  authority's `try_send`) or the broadcasts' (8 192) full, it is let go
  with a notice and joins again (catching up from its position); the
  others are not held (2026-10-26 and phase 5).
- **The pace of a connection**: presence 10 a second, locks 20,
  interests, catch-ups and working-set requests 4 a second (bursts of 8),
  the users' list broadcast at most every 100 ms.
- **Joining**: only the network's users, with their personal invitation
  or a sign-in mapped by a network admin (the general invitation admits
  nobody on a network, since phase 7); one connection per user; a user
  disabled or whose rights, invitation or groups change is let go.
- **The web client**: the same origin only for its WebSocket, the code in
  the link's fragment (never sent in a request), 16 KB before the join.
- **A job failing inside the authority** is refused to its client, the
  network read again, the server goes on (2026-10-26, M2).

## OIDC sign-in, as reviewed

`crates/ohl-server/src/oidc.rs`, with `jsonwebtoken` (the token's checks)
and `ureq` (HTTPS): the signature with the provider's keys (asymmetric
algorithms only: RS*, PS*, ES256, ES384, EdDSA — never HS* or `none`; a
key that is a shared secret refused; the key's algorithm, if it says one,
the token's), the issuer exactly, the audience (the client id), `exp`,
`nbf`, `iat` (not in the future, not older than an hour), a subject. The
keys from the provider's discovery document (its `issuer` must be the one
configured), over HTTPS only, the provider's CA or the Mozilla roots, no
redirect, 1 MB and 10 s at most; kept an hour, fetched again for an
unknown key at most every 30 s. The check runs off the authority (the
fetch may take a moment); a bad token makes the address wait as a wrong
code does. The subject signs in only as the user a network admin mapped
it to.

## The sign-in window's side (2026-11-20)

`crates/ohl-server/src/sign_in.rs` (the client's side, used by
OHL-Design's Sign in to a network window) and the server's answer to
`sign_in_options`:

- **What the server says before a join**: only whether it takes OIDC
  sign-ins, with its provider's issuer and the client id — both public
  (they are in the provider's own sign-in page). Nothing about users;
  the question costs the address its pending slot like a hello, answered
  once, then the connection goes.
- **The authorization code flow with PKCE** (S256, a 64-character
  verifier from the system's random source), a `state` and a `nonce` of
  32 random characters each, checked; the redirect on the loopback
  (`http://127.0.0.1:PORT/callback`, RFC 8252), a port for each sign-in,
  listening on 127.0.0.1 only, closed after it, at most 5 minutes, any
  other path answered 404, 16 KB of request read at most; the provider's
  endpoints from its discovery document (its `issuer` the one the server
  names), over HTTPS only (plain HTTP only to this computer: tests), no
  redirect followed, 10 s and 1 MB at most. The ID token's claims looked
  at (issuer, audience, nonce) before it is used; its signature and
  times are the server's to check, as before.
- **No password in OHL-Design**, and **tokens in memory only** — no
  keychain is used in the code base, so none is used here: never written,
  never logged (`SignedIn`'s `Debug` names no token, tested), gone when
  OHL-Design closes. A refresh token, when given, renews the ID token for
  joining again (after 50 minutes, a server taking tokens issued within
  the hour).
- **Accepted**: a program on the same computer could connect to the
  loopback port during a sign-in; without the `state` its answer is
  refused, and the code is useless without the verifier (PKCE).

## The web viewer's sign-in (2026-11-21)

`crates/ohl-server/web/signin.js`, `app.js` and the page's headers in
`web.rs` (the web viewer's "Sign in with your organisation"):

- **The page's Content-Security-Policy** gains the identity provider's
  origin in `connect-src` (beside `'self'` and the page's own `wss://`):
  the scheme, host and port of the **server's configured issuer**
  (`--oidc-issuer`), never of the request — a `Host` or `Origin` header
  changes nothing (tested) — and only for HTTPS (plain HTTP only to this
  computer: tests) and a plain host name (`web::provider_origin`; an
  issuer with other characters adds nothing, and the page then cannot
  reach the provider). Nothing else of the policy changes: scripts,
  styles and images still from the page only, no frames, no forms, no
  base. A provider whose token endpoint is on another origin than its
  issuer is not added (the page says so): **accepted**, a narrower
  policy over providers that split their endpoints.
- **The redirect URI** is the page's own address, `https://HOST:PORT/`,
  exactly; the provider must allow it (and the page's origin on its
  discovery document and token endpoint: CORS). The code comes back in
  the query of `/`, which the server answers with the page (it reads no
  query); the page takes it out of the address bar and the tab's history
  at once (`history.replaceState`); `Referrer-Policy: no-referrer` and
  `Cache-Control: no-store` as for every file. PKCE (S256, a 64-character
  verifier from `crypto.getRandomValues`), a `state` and a `nonce` of 32
  characters, checked; a sign-in older than 10 minutes refused; the ID
  token's issuer, audience and nonce looked at before it is used (its
  signature and times are the server's to check, as before).
- **Where the tokens are**: in the page's memory only (a variable of the
  page's script) — never in `localStorage`, `sessionStorage`, cookies or
  the address; gone when the tab closes or reloads. Only the one-time
  values of the sign-in under way (verifier, state, nonce, the token
  endpoint, the user name typed — no token) are kept in the tab's
  `sessionStorage` while the tab is on the provider's page, removed as
  soon as it is back (tested in Node.js with the page's own script against
  the fake provider). **Accepted**: script running in the page (an XSS)
  could read the tokens in memory, as it could the access code; the
  page's policy allows scripts from the page only, and the page writes
  what the server sends as text, never as HTML.
- The join carries the ID token over the page's WebSocket (TLS), as
  OHL-Design's; the server's checks and limits are unchanged (S8).

## The admin messages (2026-11-28)

*In active development, not approved yet.* OHL-Server's admin messages
(`ClientMsg::Admin`, `crates/ohl-server/src/admin.rs` and
`network_admin.rs`; tests `admin_tests.rs` and the app's own) reviewed
against the same attackers. Each message is asked on a joined
connection only (an invitation or a mapped sign-in: nothing before the
join), paced (8 a second, 32 at once, per connection; more are refused
with "slow down"), and refused on a connection seeing as another user.

| Message | Who may | Audited | What it could leak or do, and why it does not |
|---|---|---|---|
| `overview` | anyone managing rights somewhere, or maintaining a library; full to network admins | refusals | others get the program, the uptime and counts of the lines they see — no user names, no backup times |
| `sessions` | network admins | refusals | who is connected and what they follow — network admins see it anyway (the audit) |
| `disconnect` | network admins | yes (who, the connection, the reason) | a reason is required; one's own connection refused; the user may join again (disabling them is a rights change) |
| `log` | network admins | refusals | the server's own log lines (the newest 4000, each cut at 2000 characters): the server never logs a secret — no access code, token, password or connection string (checked: every `log` call) |
| `backups`, `backup_now`, `restore_drill` | network admins | yes (start, and the backup's own record) | the backup folder is the server's start option, never a client's path; a drill names a folder of it by name only (no `/`, `\`, leading `.`, control characters: refused, tested with `../`); a drill database equal to the network's own is refused; one long work at a time, off the authority (the server keeps serving) |
| `settings` | network admins | refusals | the database by host and name only — the password and the drill database's string never sent (tested) |
| `rights` | anyone managing rights somewhere, or maintaining a library | refusals | users, groups and roles are listed to every manager (an assignment names them); assignments only where the asker manages (`UserRights::manages`), all to network admins (tested with a line admin) |
| `change` | the rights' one function decides (`Network::rights_command`) | yes (in the same transaction, before and after) | nothing new: the command line's checks; the server reads the rights again at once (clients whose rights changed are let go) |
| `preview` | anyone managing rights somewhere, or maintaining a library | no (nothing saved) | worked out on a copy of the rights the asker could list anyway; names only users (no objects); a change the asker may not save says so |
| `invite`, `reissue` | network admins | yes (`admin.invite`, `admin.reissue_invitation`, and the user added) | the access code goes only to the network admin who asked, over the connection's TLS; the app shows it once and keeps it in memory only; days 1–3650 |
| `see_as` | anyone managing rights somewhere, or maintaining a library | yes | what both the asker and the user may see (`UserRights::and`), as the join's see-as |
| `map`, `structures`, `route_structures` | anyone managing rights somewhere, or maintaining a library | no | only routes and structures the asker may see (the rights' one function per object); a hidden route asked for by name gets the hidden refusal (no name back); at most 20 000 structures per answer |
| `libraries` | anyone managing rights somewhere, or maintaining a library | no | the store's own listing with the asker's rights (entries they may not see left out) |

**Host mode** (the server inside the app): the database's connection
string, password file and TLS are those of `ohl network serve`; the
string holding a password stays in memory (never shown: the Settings
tab parses it for its host and name), nothing of the host form is saved.
The first network admin is made only while the network has none (the
rights function's own rule). The app signs in to its own server through
the protocol with the admin's personal invitation, on 127.0.0.1.

**Accepted**: the overview's counts and the map's bounds tell a line
admin how much of the network they see (not what the rest is).
Reads are not audited (an app refreshes them every few seconds; the
audit would fill with them).

## Not reviewed here

The desktop applications, the plugins' sandbox (ADR-0003, its own
review), the web client's page itself (it shows what the server sends),
and PostgreSQL's own configuration (the owner's IT: `pg_hba.conf`, TLS on
the database's side, disk encryption).
