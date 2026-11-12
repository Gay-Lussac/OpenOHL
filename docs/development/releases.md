# Making a release

For the owner of the repository: how the files of a release are made and published. A release has the macOS disk image (made on a Mac), the Linux archive, the Windows archive and the Windows installer (made by a workflow on GitHub).

Releases of this repository are **immutable**: once a release is published, no file can be added, replaced or removed, and its tag cannot be moved or used again, even after the release is deleted. So every release starts as a **draft**, gets all its files, and is published by hand at the end.

## The steps

1. Set the version in `Cargo.toml` (`[workspace.package]`, three numbers such as `0.0.2`), run the full check (`tools/check.sh`), push.
2. Run the release workflow for the tag (below). It makes the draft release and puts the Linux and Windows files into it.
3. Make the macOS image on a Mac (below) and add it to the draft.
4. Try the files (the list under [Before publishing](#before-publishing)).
5. Edit the notes of the draft and publish it.

## The release workflow (Linux and Windows)

`.github/workflows/release.yml` runs only when started by hand: GitHub › Actions › **Release builds** › Run workflow. It asks for:

| Input | Meaning |
|---|---|
| **tag** | The tag of the release, `v` and the version (`v0.0.2`). If the tag exists, the tag is built. |
| **commit** | Only when the tag does not exist yet: the commit (full SHA) or branch to build. Publishing the draft then creates the tag at that commit, so no tag has to be pushed. |
| **name** | The title of the release; empty gives `OpenOHL <tag>`. Only used when the draft is made. |

What it does:

1. Finds the commit, then the draft release for the tag; if there is none it makes one, with notes to edit. If a release with this tag is already **published**, it stops before building anything.
2. Builds the three programs for Linux and for Windows, without tests. OHL-Design is built without its Python console (`--no-default-features`): the console would need the user's own Python of exactly the version it was built with. The notes of the draft say so.
3. Uploads the files to the draft. A file of the same name that is already there is replaced, so the workflow can be run again for the same tag as long as the release is a draft.

It never publishes. The files are also kept with the workflow run for five days (Artifacts on the run's page), in case the upload to the draft fails: they can then be added to the draft by hand without building again.

Time and cost: about 25 minutes for each system when nothing is cached, and Windows minutes count double against the Actions budget. Each job stops after 75 minutes at the latest.

### What the draft contains

| File | Contents |
|---|---|
| `OpenOHL-<tag>-windows-x86_64.msi` | The Windows installer. |
| `OpenOHL-<tag>-windows-x86_64.zip` | The same files in a folder, without an installer: unpack anywhere and start `ohl-design.exe`. |
| `OpenOHL-<tag>-linux-x86_64.tar.gz` | The Linux build, the same folder. |

Each holds `ohl-design`, `ohl-structure` and `ohl` side by side (OHL-Design starts OHL-Structure from its own folder) and, beside them, what the programs look for there: `libraries/`, `examples/`, the manual — the built book in `book/` (owner, 2026-11-12: the guide included in the application; Help and the windows' user guide buttons open it offline; the formulas of the theory pages need a connection, their script loaded from the internet) and its Markdown sources (`docs/user-guide`, `docs/tutorial`, `docs/theory`, `docs/images`) —, `LICENSE` and `README.md`.

## The Windows installer

The installer is an MSI made with the WiX Toolset from `packaging/windows/openohl.wxs`. It:

- installs for all users into `C:\Program Files\OpenOHL` (the folder can be changed), which needs administrator rights;
- puts **OHL-Design** and **OHL-Structure** into the Start menu;
- opens `.ohl` files (projects) with OHL-Design and `.ohls` files (structures) with OHL-Structure. Line files (`.ohl.json`) are not registered: Windows only looks at the last part of the name, and `.json` belongs to other programs;
- offers **Add the ohl command line to PATH**, ticked unless the user unticks it: the OpenOHL folder goes to the end of the system `PATH` and is taken off again on removal. A terminal opened afterwards finds `ohl`;
- shows up under Settings › Apps as "OpenOHL <version>", publisher OpenOHL, with the link to the repository, and is removed from there;
- replaces an older OpenOHL when a newer one is installed (the old one is removed first), and refuses to install over a newer one.

Without questions, for example from a script: `msiexec /i OpenOHL-v0.0.2-windows-x86_64.msi /qn` (add `ADDLOCAL=Main` to leave `PATH` alone); removal: `msiexec /x OpenOHL-v0.0.2-windows-x86_64.msi /qn`.

The examples are installed with the programs, where ordinary users cannot write. To work on an example, copy its folder to your documents first.

### How it is built

Three things in the workflow's Windows job, after the programs are built:

1. `packaging/windows/make_assets.py` (Python, standard library only) draws the two application icons from `assets/brand/ohl-design-icon.svg` and `ohl-structure-icon.svg` into `.ico` files (16 to 256 pixels) and writes the `LICENSE` as the installer's licence page. Nothing binary is kept in the repository. The script reads only the kind of SVG the two icons are (a filled rounded rectangle, lines with round ends, plain colours); for anything else it stops and names what it met, rather than drawing a wrong icon. It runs on a Mac as well: `python3 -I packaging/windows/make_assets.py /tmp/out`.
2. WiX Toolset 5.0.2 is installed as a .NET tool (`dotnet tool install wix --version 5.0.2`; the .NET SDK is on GitHub's Windows machines, whose own WiX is the old version 3).
3. `wix build` makes the MSI from `openohl.wxs`. The three programs are named in that file; everything else in the staged folder is taken as it is (`<Files Include="…\**">`), so there is no list of files to keep up to date.

Why version 5.0.2 and not a newer one: version 5 is the first that takes a whole folder without a list. From version 6 on, WiX asks organisations with revenue for a maintenance fee, and from version 7 on the build must state that its licence terms are accepted. Moving to a newer version is therefore a decision about those terms, not only a number to change. The source file should need no change for it.

Do not change the `UpgradeCode` or the other GUIDs in `openohl.wxs`: they are how Windows recognises an installed OpenOHL. With a new `UpgradeCode`, a new version would be installed beside the old one instead of replacing it.

Windows Installer compares only the three numbers of the version. Two builds of the same version replace each other; to make "newer" mean something, raise the version for every release.

### Not signed: what users see

The installer and the programs are not signed, because signing needs a certificate (below). The notes of the draft say so. A user who downloads the installer with a browser sees:

1. possibly a warning of the browser that the file is not commonly downloaded (keep it);
2. on starting it, a blue window **Windows protected your PC** (Microsoft Defender SmartScreen). **More info** shows the file name and "Publisher: Unknown publisher"; **Run anyway** continues;
3. the question for administrator rights, which names an unknown publisher as well.

The zip behaves the same way at the first start of each program. In organisations, SmartScreen or other rules may block unsigned programs altogether; then only an administrator can install it.

The warning does not say that anything is wrong with the file, only that Windows cannot tell who made it. Users who want to be sure can compare the file's SHA-256 with the one GitHub shows for the release file.

### What signing would need

- A **code-signing certificate** in the name of a person or an organisation whose identity the issuer has checked. The private key of such a certificate may no longer be a file: it sits on a hardware token or in a signing service, so signing in a workflow needs a service that the workflow can call. This costs money every year. Some organisations sign open-source projects free of charge on application; they sign as themselves or under their own conditions.
- Signing the three `.exe` files before they go into the installer and the archive, then the `.msi` itself (`signtool sign` with a timestamp, so the signature stays valid after the certificate runs out). In the workflow that is one step before "Stage" and one after the installer is built.
- Time: with an ordinary certificate SmartScreen still warns at first and stops once enough people have installed files signed with it. The publisher's name is shown from the first day.

Until then the honest way is the one taken here: say that the files are unsigned, and say what Windows shows.

### Known gaps on Windows

These need a small change to the applications, which has not been made:

- The two applications are built as console programs, so a terminal window opens beside each of them when it is started from the Start menu or by a file.
- The `.exe` files carry no icon of their own: Explorer shows the plain program icon for them. The Start menu, the file types and the entry under Settings › Apps show the OpenOHL icons (they come from the installer), and the window and taskbar show the icon the application sets itself.

## The macOS image

On a Mac, in the repository at the commit of the release:

```sh
tools/package-macos.sh              # this Mac's kind of processor
tools/package-macos.sh --universal  # Apple Silicon and Intel in one image
```

`--universal` needs the Intel target once (`rustup target add x86_64-apple-darwin`) and builds twice. `tools/package-macos.sh --help` lists the other options. A build from nothing takes 10 to 15 minutes. It writes into `dist/` (not in git):

- `OpenOHL-<version>-macos-<arch>.dmg`: OHL-Design.app, OHL-Structure.app and a link to Applications;
- `OpenOHL-<version>-macos-<arch>-cli.tar.gz`: the `ohl` command line with the examples and libraries.

The applications are signed ad hoc only (no Apple developer certificate): on another Mac the first start needs right-click › Open, or System Settings › Privacy & Security › Open anyway. Say so in the notes.

Add both files to the draft, on the draft's page (Edit › attach files) or from the terminal:

```sh
gh release upload v0.0.2 dist/OpenOHL-0.0.2-macos-universal.dmg dist/OpenOHL-0.0.2-macos-universal-cli.tar.gz
```

## Before publishing

On a Windows machine (a virtual one will do), with the installer from the draft:

- it installs; OHL-Design and OHL-Structure start from the Start menu;
- in OHL-Design, Help opens the manual, a new project finds the shipped libraries, and **Edit** on a structure starts OHL-Structure;
- a double click on a `.ohl` and on a `.ohls` file opens it in the right application;
- a new terminal finds `ohl` (`ohl --version`);
- installing it over the previous release replaces that one (one entry under Settings › Apps);
- removing it leaves no OpenOHL folder in Program Files, no Start-menu entries and no OpenOHL entry in `PATH`.

The step "Check the installer" in the workflow's log lists what Windows Installer's own consistency checks say about the file. It is for information: the build does not stop on it.

## Publishing

On GitHub › Releases, open the draft, edit the notes (they begin with a reminder), tick **Set as a pre-release** where that applies, and press **Publish release**. If the tag did not exist, it is created now at the commit that was built.

From then on the release cannot be changed, apart from its title and notes. A mistake in a file means a new version and a new tag. A draft, by contrast, can be deleted and made again as often as needed.
