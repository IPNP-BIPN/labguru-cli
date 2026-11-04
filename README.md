# lg — Labguru on the command line

`lg` is to [Labguru](https://www.labguru.com) what `gh` is to GitHub: `lg <object> <verb>` for projects,
folders, experiments, sections, notes, papers, attachments and tags, with readable tables, JSON for scripts,
and authentication handled for you. It works with any instance (my.labguru.com or a hosted one such as
`cle.inserm.fr`), any account and any workspace — and it publishes **your own notebook**, written as
Markdown files, to a Labguru project.

*En français : l'interface passe en français quand `LANG` ou `LC_ALL` commence par `fr`.*

This README is a tutorial: follow it from top to bottom once (about 15 minutes), then keep it as a reference.

1. [Install lg](#1-install-lg)
2. [Log in](#2-log-in)
3. [Look around](#3-look-around)
4. [Make a first change](#4-make-a-first-change)
5. [Publish your notebook](#5-publish-your-notebook)
6. [Use it in scripts](#6-use-it-in-scripts)
7. [Good to know](#7-good-to-know)
8. [When something goes wrong](#8-when-something-goes-wrong)

---

## 1. Install lg

`lg` is a single binary with no dependency. Download the archive for your system from the
[releases page](https://github.com/IPNP-BIPN/labguru-cli/releases):

| System | Archive |
|---|---|
| macOS, Apple silicon (M1…M4) | `lg-v0.1.1-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `lg-v0.1.1-x86_64-apple-darwin.tar.gz` |
| Linux, x86-64 | `lg-v0.1.1-x86_64-unknown-linux-musl.tar.gz` |
| Linux, ARM64 | `lg-v0.1.1-aarch64-unknown-linux-musl.tar.gz` |
| Windows | `lg-v0.1.1-x86_64-pc-windows-msvc.zip` |

On macOS or Linux, unpack it and put `lg` on your `PATH`:

```bash
tar xzf lg-v0.1.1-aarch64-apple-darwin.tar.gz
mkdir -p ~/.local/bin && mv lg-v0.1.1-aarch64-apple-darwin/lg ~/.local/bin/
xattr -d com.apple.quarantine ~/.local/bin/lg 2>/dev/null   # macOS: the binary is not signed
lg --version
```

If `lg --version` answers `command not found`, add `~/.local/bin` to your `PATH` (for zsh:
`echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc`, then open a new terminal).

On Windows, unzip the archive and move `lg.exe` to a folder listed in your `PATH`.

**Or install it with Cargo** (Rust 1.88 or later); the crate is `labguru-cli`, the command is `lg`:

```bash
cargo install labguru-cli
```

**Optional:** tab completion and the manual page (also included in the archives):

```bash
lg completion zsh > ~/.zfunc/_lg          # or bash, fish, powershell, elvish
lg man > ~/.local/share/man/man1/lg.1     # then: man lg
```

## 2. Log in

```bash
lg auth login --hostname cle.inserm.fr      # without --hostname: my.labguru.com
```

`lg` asks for your e-mail and password **once**, exchanges them for a token and keeps only the token, in the
system keyring (macOS Keychain, Windows Credential Manager, Secret Service on Linux). The password is never
stored. Without a keyring, the token goes to `~/.config/lg/config.toml`, readable by you only.

**If your institution signs you in through SSO** (or the password is refused), use a token instead: create
one in Labguru, in your account settings, copy it, then:

```bash
pbpaste | lg auth login --hostname cle.inserm.fr --with-token      # macOS
lg auth login --hostname cle.inserm.fr --with-token < token.txt    # anywhere
```

This way the token is never shown on screen nor kept in your shell history.

Check that it worked:

```bash
lg auth status
```

```
cle.inserm.fr
  * default — valid token — system keyring
    API v1 only, 236 documented endpoints
```

The last line is what `lg` learned from the instance's API documentation at login: a command that your
instance does not support says so, instead of failing halfway.

> **macOS:** the first time `lg` reads its token, macOS asks whether `lg` may use the keychain item
> « lg:cle.inserm.fr ». Click **Always Allow**. After installing a new version of `lg`, macOS asks once more.

## 3. Look around

Every command has the form `lg <object> <verb>`. Start with what you have:

```bash
lg project list
```

```
ID  TITLE         START       UPDATED
1   Mouse brain   2026-08-28  2026-11-04 08:09
5   Cell culture  2026-11-04  2026-11-04 14:10
```

Then go down: folders of a project, experiments of a folder, one experiment with its sections as text.

```bash
lg folder list --project 5
lg experiment list --project 5
lg experiment view 171            # fields, then each section as plain text
lg experiment view 171 --web      # the same page in your browser
lg browse folder 109              # any object, in the browser
```

Every list and view can give JSON. `--json` alone lists the available fields; `--jq` filters with jq syntax.

```bash
lg experiment list --project 5 --json id,title,start_date
lg experiment list --project 5 --json | head            # which fields exist?
lg project list --jq '.[] | [.id, .title] | @tsv'
```

`lg <object> --help` shows all the verbs, with examples.

## 4. Make a first change

**Do this in a test project**, so that you can try everything without touching real work:

```bash
lg project create --title "lg sandbox"                      # prints the new id, say 7
lg folder create --project 7 --title "Trials" --status in-progress
lg experiment create --project 7 --folder 34 --title "Run 1" --start 2026-11-01 --end 2026-11-03
```

Add text. Sections take Markdown, which `lg` turns into HTML (tables, lists, checkboxes, `code`, links):

```bash
lg section list --experiment 56
echo '**Done.** Yield: 82 %' | lg section edit 78              # a section's id, from the list above
lg section create --experiment 56 --name "Notes" --body-file notes.md
```

Attach files, tag, move:

```bash
lg attachment upload figure.png --experiment 56
lg attachment download 90 --output figure.png
lg tag create RNA-seq --experiment 56
lg experiment move 56 --folder 35
lg experiment edit 56 --end 2026-11-05            # the Gantt duration follows
```

Deleting asks for confirmation (`--yes` to skip it in scripts). `lg` never deletes anything you did not name.

## 5. Publish your notebook

This is what `lg` was written for: you keep your lab notebook as Markdown files, next to your data and code,
and `lg` keeps a Labguru project identical to it.

### 5.1 Create the notebook

```bash
lg notebook init my-notebook --project "My project"
cd my-notebook
```

```
my-notebook/
  lg.toml                  Labguru project name, data directories, folder layout
  project.md               project description
  references.bib           papers, added to the project's Key Papers (optional)
  notes/meeting.md         project notes (optional)
  attachments/             files attached to the project (optional)
  data/                    your data: file times give the hours of each day (optional)
  example/                 one notebook folder = one Labguru folder
    README.md              the folder's summary
    2026-01-15-first-run.md      one entry = one experiment
    2026-02-01-planned-run.md
    figures/example.png
```

`project` in `lg.toml` is the name of the Labguru project: it is created if it does not exist yet. Point
`data_dirs` at the folders where your instruments or pipelines write files.

### 5.2 Write an entry

```bash
lg notebook new example "Library QC"      # creates example/2026-11-04-library-qc.md
```

An entry is a title and up to three sections, which become the experiment's sections in Labguru:

```markdown
# Library QC

## Description
Why, and what we expect.

## Procedure
1. Quantify the libraries (`Qubit`).
2. Check the size profile.

| Library | ng/µl |
|---|---|
| L1 | 12.4 |

## Results
04/11/2026: all libraries pass. ![profile](figures/profile.png)
```

French headings work too (`## Procédure`, `## Résultats`). Local images are uploaded and shown at full size.

### 5.3 Look, then publish

```bash
lg notebook push              # dry run: shows what would be created (+), changed (~), moved (→), removed (-)
lg notebook push --apply      # writes to Labguru
lg notebook push              # run it again: “Labguru is up to date.”
lg notebook check             # reads Labguru back and reports any difference
```

Edit your files and push again whenever you like: only what changed is sent.

### 5.4 How entries are filed

- **Folders.** Each notebook folder becomes a Labguru folder (its README is the Description). Entries go into
  top-level folders “`<folder> — Completed / In Progress / Planned`”; a status folder that becomes empty is
  removed. Labels and their one-line descriptions are set in `lg.toml`; `layout.style = "single"` keeps one
  folder instead.
- **Status.** An entry with no results is *planned*; with results it is *in progress*; Results starting with
  “Not done” / “Non réalisé” keep it *planned*. To mark it *done*, add a header to the entry:

  ```markdown
  ---
  status: done            # done | in progress | planned  (or terminée | en cours | prévue)
  start: 2026-10-19       # optional: otherwise taken from the text
  end: 2026-10-21
  ---
  # Library QC
  ```

- **Dates.** Without `start`/`end`, an experiment runs from the first to the last date cited in the entry
  (`DD/MM/YYYY`, `YYYY-MM-DD`, or `DD/MM` — the year then comes from the file name), never after today.
  “1/10” is a dilution, not a date; snapshots such as “as of …”, “au 03/11”, “Figures du 02/11” are ignored.
  The time of day is that of the first (or last) file modified that day under `data_dirs`.
- **Safety.** Objects are found by an `external_uuid` (`lg:<project>/<folder>/<file>`), else by title when no
  other entry owns that object. Nothing is duplicated, no experiment is ever deleted, and a text block holding
  inline comments from your team in Labguru is never overwritten (`lg` warns instead).

Complete examples: [`examples/notebook`](examples/notebook) (English) and [`examples/cahier`](examples/cahier)
(French).

## 6. Use it in scripts

Output follows gh: a table on a terminal, tab-separated values when piped, JSON with `--json`, filtered with
`--jq`, shaped with `--template '{{.id}} {{.title}}\n'`. `NO_COLOR` is honoured.

| Exit code | Meaning |
|---|---|
| 0 | success |
| 1 | error (or differences found by `lg notebook check`) |
| 2 | wrong usage |
| 4 | not logged in, or token refused |

`lg api` reaches any endpoint, with the token added for you:

```bash
lg api projects --paginate --jq '.[].title'
lg api -X PUT experiments/56 -f 'item[title]=New title'
lg api experiments --input new.json             # POST a JSON body
lg api v2/experiments -f page_size=10
```

In CI or on a server, set `LABGURU_TOKEN` and `LABGURU_HOST` instead of logging in.

Shortcuts and settings:

```bash
lg alias set el 'experiment list --project 5'    # then: lg el
lg config set browser firefox
lg config set prompt disabled
```

Several hosts and accounts can coexist, as with gh:

```bash
lg auth login --hostname my.labguru.com          # a second host
lg auth login --account 42                       # another workspace of the same login
lg auth switch                                   # change the active account
lg auth logout --user me@lab.org
```

## 7. Good to know

Some things the Labguru API cannot do. `lg` says so instead of failing silently:

- The *Successful / Failed* flag and signatures are set in the web page (`lg experiment flag` and
  `lg experiment sign` explain how). **`lg` never signs anything.**
- A project's description can be set only when the project is created.
- A folder's start date can be set only when the folder is created.
- A section has no web page of its own: open its experiment.
- Deleting a project needs API v2 (my.labguru.com has it, some hosted instances do not).

## 8. When something goes wrong

| Message | What to do |
|---|---|
| `Not logged in to <host>` | `lg auth login --hostname <host>` |
| `<host> refused the token` | the token expired or was revoked: log in again |
| `refused this e-mail and password` | check them; with SSO, log in with a token (section 2) |
| `Standard input holds several words or lines` | copy only the token, then run the command again |
| `HTTP 422 … Labguru refused the values` | check the values sent (dates, ids, file); the message shows Labguru's answer |
| `… is not supported by the API of <host>` | your instance does not have it; `lg api` lets you try anyway |
| macOS asks for the keychain at every run | click **Always Allow** (asked again after each update) |

Set `LG_DEBUG=1` to print each request (method and path only — never the token).

## Under the hood

The quirks of the Labguru API are handled once, in [`src/api.rs`](src/api.rs) and
[`src/labguru.rs`](src/labguru.rs): the token in the query for GET (and for every API v2 request) and in the
JSON body otherwise, the `item` envelope, PUTs that answer with an empty body, retries of reads on 429/5xx
(after 5 s, 30 s, 120 s; writes are never sent twice), nested folders whose `ancestry` is cut at creation,
folder status as `completed` / `current` / `compleation_date`, experiments requiring a folder, text blocks
updated with `data` at the root, the HTML Labguru rewrites (`<br />`, `&quot;`, checkboxes), full-size image
URLs, paper links, tag removal.

## Development

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Tests run against an in-memory Labguru server (`tests/fake`); they never touch a real instance.
`cargo test -- --ignored` also checks the system keyring.

## License

MIT
