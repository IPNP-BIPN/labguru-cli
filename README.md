# lg — Labguru on the command line

`lg` is to [Labguru](https://www.labguru.com) what `gh` is to GitHub: `lg <object> <verb>`, readable tables in a
terminal, TSV or JSON for scripts, a raw `lg api` command, and authentication handled for you. It works with
any instance (my.labguru.com or a hosted one such as `cle.inserm.fr`), any account and any workspace — and it
can publish **your own notebook**, written as Markdown files, to a Labguru project.

*En français : l'interface passe en français quand `LANG` ou `LC_ALL` commence par `fr`.*

## Install

One binary, no dependency.

- **Download** the archive for macOS, Linux or Windows from the
  [releases page](https://github.com/IPNP-BIPN/labguru-cli/releases), unpack it and put `lg` (or `lg.exe`) on your `PATH`.
- **Or build it** with Rust (1.88 or later):

  ```bash
  cargo install --git https://github.com/IPNP-BIPN/labguru-cli
  ```

Then, optionally:

```bash
lg completion zsh > ~/.zfunc/_lg        # or bash, fish, powershell, elvish
lg man > /usr/local/share/man/man1/lg.1
```

## First login

```bash
lg auth login --hostname cle.inserm.fr      # or my.labguru.com (the default)
```

`lg` asks for your e-mail and password **once**, exchanges them for a token and stores only the token, in
the system keyring (macOS Keychain, Windows Credential Manager, Secret Service on Linux). Without a keyring,
the token goes to `~/.config/lg/config.toml`, readable by you only (mode 600). The password is never stored.

If your institution uses single sign-on, create a token in Labguru (*Account › API token*) and run:

```bash
lg auth login --hostname cle.inserm.fr --with-token < token.txt
```

Several hosts and accounts can coexist, as with gh:

```bash
lg auth status                  # hosts, accounts, token validity, what the instance's API offers
lg auth switch                  # change the active account
lg auth login --account 42      # another workspace of the same login
lg auth logout --user me@lab.org
```

`LABGURU_TOKEN` and `LABGURU_HOST` override the stored values (handy in CI). The token is never printed:
not in output, logs, errors or URLs.

At login, `lg` reads the instance's API documentation (`/api-docs`) to learn whether it has API v2 and which
endpoints exist. A command the instance does not support says so instead of failing halfway.

## Everyday use

```bash
lg project list
lg folder list --project 12
lg experiment list --project 12 --json id,title,start_date
lg experiment view 56                      # fields and sections, as text
lg experiment view 56 --web                # in the browser
lg experiment create --project 12 --folder 34 --title "Run 1" --start 2026-10-19 --end 2026-10-21
lg experiment move 56 --folder 35
lg section create --experiment 56 --name Results --body-file results.md   # Markdown → HTML
echo "**done**" | lg section edit 78
lg note create --project 12 --title "Meeting" --body-file meeting.md
lg paper add --bib references.bib --project 12
lg attachment upload figure.png --experiment 56
lg attachment download 90 --output figure.png
lg tag create RNA-seq --experiment 56
lg browse experiment 56
```

Output follows gh: a table on a terminal, tab-separated values when piped, JSON with `--json fields`,
filtered with `--jq` (jq syntax), or shaped with `--template '{{.id}} {{.title}}\n'`. `NO_COLOR` is honoured.
Exit codes: `0` success, `1` error, `2` usage error, `4` authentication required.

`lg api` reaches any endpoint, with the token added for you:

```bash
lg api projects --paginate --jq '.[].title'
lg api -X PUT experiments/56 -f 'item[title]=New title'
lg api v2/experiments -f page_size=10
```

Shortcuts and settings: `lg alias set el 'experiment list --project 12'`, `lg config set prompt disabled`,
`lg config set browser firefox`.

What the Labguru API cannot do, `lg` says plainly: the *Successful / Failed* flag and signatures are set in the
web page (`lg experiment flag`, `lg experiment sign` explain it). `lg` never signs anything.

## Publish your notebook in five minutes

1. **Create it** (1 min):

   ```bash
   lg notebook init my-notebook --project "My project"
   cd my-notebook
   ```

   ```
   my-notebook/
     lg.toml                  host, Labguru project name, data directories, folder layout
     projet.md                project description (# title)        — or project.md
     references.bib           Key Papers (optional)
     notes/*.md               project notes (optional)
     pieces/*                 project attachments (optional)        — or attachments/
     <folder>/README.md       one Labguru folder and its summary
     <folder>/<entry>.md      one experiment: # title, ## Description, ## Procedure, ## Results
   ```

2. **Write** (3 min). `lg notebook new qc "Library QC"` creates an empty entry in the right format. An entry
   may start with a header:

   ```markdown
   ---
   status: in progress        # done | in progress | planned (or terminée | en cours | prévue)
   start: 2026-10-19
   end: 2026-10-21
   ---
   # Library QC

   ## Description
   ## Procedure
   ## Results
   ```

   French headings (`## Procédure`, `## Résultats`) work too. Local images (`![](figures/a.png)`) are attached
   to the experiment and shown at full size; tables are kept; inline `code` is rendered in a monospace font.

3. **Look, then publish** (1 min):

   ```bash
   lg notebook push            # dry run: what would be created, changed or moved
   lg notebook push --apply    # write to Labguru
   lg notebook check           # read Labguru back and report differences
   ```

How entries are filed:

- Each `<folder>` becomes a Labguru folder (its README is the Description), plus top-level folders
  “`<folder> — Completed / In Progress / Planned`” (labels and one-sentence descriptions set in `lg.toml`),
  deleted when they become empty. Set `layout.style = "single"` to keep one folder.
- The status of an entry is its header status; otherwise Results starting with *Not done* / *Non réalisé*
  mean planned, other Results mean in progress, and no Results mean planned.
- Without header dates, an entry runs from the first to the last day it cites (`DD/MM[/YYYY]`, `YYYY-MM-DD`),
  up to today, ignoring snapshots (“as of …”, “au DD/MM”, “Figures du DD/MM”). The time of day is that of the
  first (or last) file modified that day under the `data_dirs` of `lg.toml`, in the machine's time zone; files
  stamped exactly 00:00:00 or 23:59:59 are ignored. The Gantt duration counts both ends.
- Pushing is idempotent: objects are found by `external_uuid` (`lg:<project>/<folder>/<file>`), else by title.
  Nothing is duplicated, no experiment is ever deleted, and a text block holding inline comments from your team
  in Labguru is never overwritten.

An example notebook is in [`examples/notebook`](examples/notebook) (English) and
[`examples/cahier`](examples/cahier) (French).

## Under the hood

The quirks of the Labguru v1 API are handled once, in [`src/api.rs`](src/api.rs) and
[`src/labguru.rs`](src/labguru.rs): token in the query for GET and in the JSON body otherwise, the `item`
envelope, PUTs that answer with an empty body, project descriptions set only at creation, nested folders whose
`ancestry` is cut to one level at creation, retries on 429/5xx (after 5 s, 30 s, 120 s), folder start dates
accepted only at creation, folder descriptions stored as a section, folder status as `completed` / `current` /
`compleation_date`, experiments requiring a folder, text blocks updated with `data` at the root, full-size image
URLs, paper links, tag removal.

Set `LG_DEBUG=1` to print each request (method and path only).

## Development

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Tests run against an in-memory Labguru server (`tests/fake`); they never touch a real instance.

## License

MIT
