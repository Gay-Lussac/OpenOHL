# Signing in to a network

*Built 2026-11-20, in active development, not approved yet*
([decisions](../development/decisions.md#2026-11-20-country-server-the-rest-of-the-exports-thread-and-the-sign-in-window),
[a network's server](server.md#a-networks-server-in-active-development)).

**File › Join a network** opens the **Sign in to a network** window first.
You join a network with the **invitation** its administrator gave you,
or — when the network's server offers it — by **signing in with your
organisation's account** in your browser. Once joined, the **Join a
network** window opens, where you pick your working set
([Working sets on a network](working-set.md)).

## Sign in to a network

![The Sign in to a network window: the invitation of a local test server with its code, "Personal invitation, valid until 2027-02-18." in green, the user name tester, the heading Sign in with your organisation with "This network also takes your organisation's sign-in (127.0.0.1:65378)." and its button, then the buttons Join with the invitation and Cancel](../images/user-guide/sign-in-to-a-network.png)

**What it is for**: to join a network's server as yourself, with your
personal invitation or your organisation's account.

**How to use it**:

1. **File › Join a network**.
2. Paste your **invitation** (`ohl://HOST:PORT/CODE/FINGERPRINT`). The
   window says at once until when it is valid, or that it has expired.
3. Check your **user name** (your computer's login name is filled in).
4. Either press **Join with the invitation** (or Enter), or, when the
   window says the network also takes your organisation's sign-in, press
   **Sign in with your organisation ↗**: your browser opens your
   organisation's sign-in page; sign in there as you always do; the
   browser then says *Back to OHL-Design: you may close this tab*, and
   the window says **Signed in as …**. Press **Join with your sign-in**.
5. The **Join a network** window opens.

**The window**:

| Field | What it does |
|---|---|
| Invitation | Your invitation from the network's administrator: the server's address, your access code, the server's certificate. Under it: *Personal invitation, valid until …* (green), *Invitation without an end* (valid until reissued), or *This invitation expired on …* (red). To sign in with your organisation, an invitation's address and certificate are enough (its code is not used). |
| User name | Your user name on the network — the one your invitation, or your organisation's account, is for. |
| Sign in with your organisation | Asked of the server as soon as the invitation gives its address: *This network also takes your organisation's sign-in (provider)* with its button, or *This network takes invitations only*. While you sign in in the browser: *Finish signing in in your browser*; then *Signed in as …*. |
| **Join with the invitation** / **Join with your sign-in** | Joins (Enter). Greyed while the invitation or the user name is missing, or while you sign in in the browser; its hover says why. |
| **Cancel** | Closes the window (Escape); a sign-in in the browser under way is stopped. Nothing changes. |

**Good to know**:

- **OHL-Design never sees your password.** Your organisation's sign-in
  happens in your own browser; it answers OHL-Design on this computer
  only (a port opened for that sign-in, closed after it), with a one-time
  code that only this sign-in can use (PKCE). What it gives back — an ID
  token, and a token to renew it when the provider gives one — is **kept
  in memory only**: never written to disk, never logged, gone when
  OHL-Design closes. Joining again after the connection was lost uses
  it, renewed when it is older than 50 minutes; when it cannot be
  renewed, sign in again.
- **An expired invitation**: personal invitations end after a number of
  days — **90 by default** since 2026-11-20 (`ohl network serve
  --invite-days N`; `0`: until reissued). Ask a network admin for a new
  one, or sign in with your organisation when the network offers it.
- **A reissued invitation**: when a network admin gives you a new
  invitation, every older one stops working (and a user connected with
  one is let go). The window says so — *This invitation was reissued … use
  your newest invitation* — with the server's own words below it.
- **Your organisation's account not on the network yet**: the server
  refuses it until a network admin maps it to your user
  (`ohl network rights map-identity`); the window says so.
- The sign-in is asked of the server before joining: a server of before
  2026-11-20 answers nothing, and the window then offers the invitation
  only.
- The web client does not offer the organisation's sign-in yet (it
  joins with an invitation).
