# Security policy

## Reporting a vulnerability

Please **do not report security problems in public issues, discussions
or pull requests.**

Report them privately through GitHub instead:

1. Open the repository's **Security** tab.
2. Choose **Report a vulnerability**.
3. Describe the problem: what is affected (OHL-Design, OHL-Structure,
   the `ohl` command line, the collaboration server, the plugin host, a
   file format), the version or commit, how to reproduce it, and what an
   attacker could do with it.

Only the maintainers can read the report. You will get an answer in the
same place; please allow a few working days, as OpenOHL is maintained by
a small team.

If you cannot use GitHub's private reporting, open a public issue that
says only that you have a security problem to report, **without any
detail**, and the maintainers will arrange a private channel.

## What happens next

- We confirm that we received the report and whether we can reproduce it.
- We agree with you on a date for the fix and its publication.
- The fix is released, and a GitHub security advisory describes the
  problem once users can update. You are credited unless you prefer not
  to be.

Please give us a reasonable time to fix the problem before you make it
public.

## Supported versions

OpenOHL is in active development and has only pre-releases. Security
fixes go into the `main` branch and the next release; older releases are
not patched. Please check that the problem still exists on `main` before
reporting it.

## In scope

- The applications and the command line in this repository.
- The collaboration server and its sessions (invitations, access codes,
  certificates, edit rights).
- The plugin host: a plugin escaping its sandbox, reaching files or the
  network it was not allowed, or getting more time or memory than the
  user allowed.
- Reading project, line, library or exchange files: a crafted file that
  crashes the program, runs code or reads or writes files elsewhere.

## Not in scope

- Plugins that are not part of this repository: report them to their
  authors (the plugin host's sandbox is in scope).
- Engineering results that look wrong but have no security impact:
  please open a normal issue for those.
- Problems that need an attacker to already control your computer or
  your user account.
