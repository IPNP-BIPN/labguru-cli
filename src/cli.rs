//! Command-line definitions (clap derive). Help texts are translated by i18n::localize.

use crate::output::OutArgs;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "lg", version, about = "Work with Labguru from the command line", after_help = EX_ROOT, propagate_version = true)]
pub struct Cli {
    /// Labguru host (e.g. my.labguru.com, cle.inserm.fr)
    #[arg(long, global = true, value_name = "HOST", env = "LABGURU_HOST", hide_env = true)]
    pub hostname: Option<String>,
    #[command(subcommand)]
    pub cmd: Cmd,
}

const EX_ROOT: &str = "Examples:
  lg auth login --hostname cle.inserm.fr
  lg project list
  lg experiment view 1234 --json title,start_date
  lg notebook init my-notebook && cd my-notebook && lg notebook push
  lg api projects --paginate --jq '.[].title'";

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Log in, log out, and switch accounts
    #[command(subcommand)]
    Auth(AuthCmd),
    /// Manage projects
    #[command(
        subcommand,
        after_help = "Examples:\n  lg project list\n  lg project create --title \"My project\" --description \"Aim: …\"\n  lg project view 12 --web"
    )]
    Project(ProjectCmd),
    /// Manage folders (milestones)
    #[command(
        subcommand,
        after_help = "Examples:\n  lg folder list --project 12\n  lg folder create --project 12 --title \"QC\" --status in-progress\n  lg folder edit 34 --status completed"
    )]
    Folder(FolderCmd),
    /// Manage experiments
    #[command(
        subcommand,
        after_help = "Examples:\n  lg experiment list --project 12\n  lg experiment create --project 12 --folder 34 --title \"Run 1\" --start 2026-10-19 --end 2026-10-21\n  lg experiment move 56 --folder 35"
    )]
    Experiment(ExperimentCmd),
    /// Manage sections of experiments, folders and projects
    #[command(
        subcommand,
        after_help = "Examples:\n  lg section list --experiment 56\n  lg section create --experiment 56 --name Results --body-file results.md\n  echo '**done**' | lg section edit 78"
    )]
    Section(SectionCmd),
    /// Manage notes
    #[command(
        subcommand,
        after_help = "Examples:\n  lg note create --project 12 --title \"Meeting\" --body-file meeting.md"
    )]
    Note(NoteCmd),
    /// Manage papers (Key Papers)
    #[command(
        subcommand,
        after_help = "Examples:\n  lg paper add --bib references.bib --project 12\n  lg paper list --json name,doi"
    )]
    Paper(PaperCmd),
    /// Upload, download and list attachments
    #[command(
        subcommand,
        after_help = "Examples:\n  lg attachment upload figure.png --experiment 56\n  lg attachment list --experiment 56\n  lg attachment download 90 --output figure.png"
    )]
    Attachment(AttachmentCmd),
    /// Add, remove and list tags
    #[command(
        subcommand,
        after_help = "Examples:\n  lg tag create RNA-seq --experiment 56\n  lg tag delete RNA-seq --experiment 56"
    )]
    Tag(TagCmd),
    /// Open an object in the web browser
    #[command(after_help = "Examples:\n  lg browse experiment 56\n  lg browse project 12")]
    Browse {
        /// Object type
        kind: Kind,
        /// Object id
        id: u64,
    },
    /// Make an authenticated Labguru API request
    #[command(after_help = "The token is added for you: in the query for GET, in the JSON body otherwise.
Paths without a version use /api/v1 (and .json).

Examples:
  lg api projects --paginate --jq '.[].title'
  lg api experiments/56
  lg api -X PUT experiments/56 -f item[title]=\"New title\"
  lg api v2/experiments -f page_size=10")]
    Api(ApiArgs),
    /// Create command shortcuts
    #[command(subcommand, after_help = "Examples:\n  lg alias set el 'experiment list --project 12'\n  lg el")]
    Alias(AliasCmd),
    /// Manage lg settings
    #[command(
        subcommand,
        after_help = "Examples:\n  lg config set prompt disabled\n  lg config set browser firefox"
    )]
    Config(ConfigCmd),
    /// Publish your own notebook (Markdown files) to Labguru
    #[command(
        subcommand,
        after_help = "Examples:\n  lg notebook init my-notebook\n  lg notebook new qc \"Library QC\"\n  lg notebook push            # dry run\n  lg notebook push --apply\n  lg notebook check"
    )]
    Notebook(NotebookCmd),
    /// Generate shell completion scripts
    #[command(
        after_help = "Examples:\n  lg completion zsh > ~/.zfunc/_lg\n  lg completion bash > /etc/bash_completion.d/lg\n  lg completion powershell >> $PROFILE"
    )]
    Completion {
        /// Shell
        shell: clap_complete::Shell,
    },
    /// Generate the man page
    #[command(after_help = "Examples:\n  lg man > lg.1 && man ./lg.1")]
    Man {
        /// Directory where to write lg.1 (default: standard output)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Project,
    Folder,
    Experiment,
    Note,
    Paper,
}

#[derive(Subcommand, Debug)]
pub enum AuthCmd {
    /// Log in to a Labguru host
    #[command(
        after_help = "Without --with-token, lg asks for your e-mail and password once and stores only the token.

Examples:
  lg auth login
  lg auth login --hostname cle.inserm.fr
  lg auth login --hostname my.labguru.com --with-token < token.txt"
    )]
    Login {
        /// Read a token from standard input
        #[arg(long)]
        with_token: bool,
        /// E-mail of the account
        #[arg(long)]
        user: Option<String>,
        /// Workspace (account) id, when your login has several
        #[arg(long)]
        account: Option<i64>,
    },
    /// Show the hosts and accounts you are logged in to
    Status,
    /// Log out of a host or account
    Logout {
        /// Account to use
        #[arg(long)]
        user: Option<String>,
    },
    /// Switch the active account or host
    Switch {
        /// Account to use
        #[arg(long)]
        user: Option<String>,
    },
}

#[derive(Args, Debug, Clone)]
pub struct ListArgs {
    /// Maximum number of results
    #[arg(long, short = 'L', default_value_t = 100)]
    pub limit: usize,
    /// Open in the web browser
    #[arg(long, short = 'w')]
    pub web: bool,
    #[command(flatten)]
    pub out: OutArgs,
}

#[derive(Args, Debug, Clone)]
pub struct ViewArgs {
    /// Object id
    pub id: u64,
    /// Open in the web browser
    #[arg(long, short = 'w')]
    pub web: bool,
    #[command(flatten)]
    pub out: OutArgs,
}

#[derive(Args, Debug, Clone)]
pub struct DeleteArgs {
    /// Object id
    pub id: u64,
    /// Skip the confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct BodyArgs {
    /// Body, in Markdown
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Read the body from a file ('-' for standard input)
    #[arg(long, short = 'F', value_name = "FILE")]
    pub body_file: Option<PathBuf>,
}

/// One container: --project, --folder or --experiment (and for tags/attachments, --note / --paper).
#[derive(Args, Debug, Clone)]
#[group(required = true, multiple = false)]
pub struct Target {
    /// Project id
    #[arg(long)]
    pub project: Option<u64>,
    /// Folder id
    #[arg(long)]
    pub folder: Option<u64>,
    /// Experiment id
    #[arg(long)]
    pub experiment: Option<u64>,
    /// Note id
    #[arg(long)]
    pub note: Option<u64>,
    /// Paper id
    #[arg(long)]
    pub paper: Option<u64>,
}

#[derive(Subcommand, Debug)]
pub enum ProjectCmd {
    /// List objects
    List(ListArgs),
    /// Show one object
    View(ViewArgs),
    /// Create an object
    Create {
        /// Title
        #[arg(long)]
        title: String,
        /// Description, in Markdown
        #[arg(long)]
        description: Option<String>,
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: Option<String>,
    },
    /// Edit an object
    Edit {
        /// Object id
        id: u64,
        /// Title
        #[arg(long)]
        title: Option<String>,
        /// Description, in Markdown
        #[arg(long)]
        description: Option<String>,
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: Option<String>,
    },
    /// Delete an object
    Delete(DeleteArgs),
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum FolderStatus {
    Completed,
    InProgress,
    Planned,
}

#[derive(Subcommand, Debug)]
pub enum FolderCmd {
    /// List objects
    List {
        /// Project id
        #[arg(long)]
        project: Option<u64>,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Show one object
    View(ViewArgs),
    /// Create an object
    Create {
        /// Project id
        #[arg(long)]
        project: u64,
        /// Title
        #[arg(long)]
        title: String,
        /// Parent folder id (default: top level)
        #[arg(long)]
        parent: Option<u64>,
        /// Folder status
        #[arg(long)]
        status: Option<FolderStatus>,
        /// Start date (YYYY-MM-DD); Labguru only accepts it at creation
        #[arg(long)]
        start: Option<String>,
        /// Description, in Markdown
        #[arg(long)]
        description: Option<String>,
    },
    /// Edit an object
    Edit {
        /// Object id
        id: u64,
        /// Title
        #[arg(long)]
        title: Option<String>,
        /// Folder status
        #[arg(long)]
        status: Option<FolderStatus>,
        /// Description, in Markdown
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete an object
    Delete(DeleteArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct Dates {
    /// Start date (YYYY-MM-DD)
    #[arg(long)]
    pub start: Option<String>,
    /// End date (YYYY-MM-DD)
    #[arg(long)]
    pub end: Option<String>,
    /// Duration in days, for the Gantt chart
    #[arg(long)]
    pub duration: Option<i64>,
}

#[derive(Subcommand, Debug)]
pub enum ExperimentCmd {
    /// List objects
    List {
        /// Project id
        #[arg(long)]
        project: Option<u64>,
        /// Folder id
        #[arg(long)]
        folder: Option<u64>,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Show one object
    View(ViewArgs),
    /// Create an object
    Create {
        /// Project id
        #[arg(long)]
        project: u64,
        /// Folder id
        #[arg(long)]
        folder: u64,
        /// Title
        #[arg(long)]
        title: String,
        #[command(flatten)]
        dates: Dates,
    },
    /// Edit an object
    Edit {
        /// Object id
        id: u64,
        /// Title
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        dates: Dates,
    },
    /// Move an experiment to another folder
    Move {
        /// Object id
        id: u64,
        /// Folder id
        #[arg(long)]
        folder: u64,
    },
    /// Delete an object
    Delete(DeleteArgs),
    /// Explain why flags (Successful / Failed) cannot be set here
    Flag {
        /// Object id
        id: u64,
    },
    /// Explain why experiments cannot be signed here
    Sign {
        /// Object id
        id: u64,
    },
}

#[derive(Subcommand, Debug)]
pub enum SectionCmd {
    /// List objects
    List {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Show one object
    View(ViewArgs),
    /// Create an object
    Create {
        #[command(flatten)]
        target: Target,
        /// Section name
        #[arg(long)]
        name: String,
        #[command(flatten)]
        body: BodyArgs,
    },
    /// Edit an object
    Edit {
        /// Object id
        id: u64,
        /// Section name
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        body: BodyArgs,
    },
    /// Delete an object
    Delete(DeleteArgs),
}

#[derive(Subcommand, Debug)]
pub enum NoteCmd {
    /// List objects
    List {
        /// Project id
        #[arg(long)]
        project: Option<u64>,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Show one object
    View(ViewArgs),
    /// Create an object
    Create {
        /// Project id
        #[arg(long)]
        project: u64,
        /// Title
        #[arg(long)]
        title: String,
        #[command(flatten)]
        body: BodyArgs,
    },
    /// Edit an object
    Edit {
        /// Object id
        id: u64,
        /// Title
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        body: BodyArgs,
    },
    /// Delete an object
    Delete(DeleteArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct PaperFields {
    /// Authors
    #[arg(long)]
    pub authors: Option<String>,
    /// Journal
    #[arg(long)]
    pub journal: Option<String>,
    /// Publication date (YYYY-MM-DD)
    #[arg(long)]
    pub date: Option<String>,
    /// DOI (e.g. 10.1038/171737a0)
    #[arg(long)]
    pub doi: Option<String>,
    /// Web address of the paper
    #[arg(long)]
    pub url: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum PaperCmd {
    /// List objects
    List(ListArgs),
    /// Show one object
    View(ViewArgs),
    /// Create an object
    Create {
        /// Title
        #[arg(long)]
        title: String,
        #[command(flatten)]
        fields: PaperFields,
    },
    /// Add papers from a BibTeX file
    Add {
        /// BibTeX file
        #[arg(long)]
        bib: PathBuf,
        /// Link the papers to this project
        #[arg(long)]
        project: Option<u64>,
    },
    /// Edit an object
    Edit {
        /// Object id
        id: u64,
        /// Title
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        fields: PaperFields,
    },
    /// Delete an object
    Delete(DeleteArgs),
}

#[derive(Subcommand, Debug)]
pub enum AttachmentCmd {
    /// List objects
    List {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Show one object
    View(ViewArgs),
    /// Upload a file
    #[command(visible_alias = "create")]
    Upload {
        /// File to upload
        file: PathBuf,
        #[command(flatten)]
        target: Target,
        /// Title
        #[arg(long)]
        title: Option<String>,
    },
    /// Download a file
    Download {
        /// Attachment id
        id: u64,
        /// Where to write the file (default: its name, '-' for standard output)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    /// Delete an object
    Delete(DeleteArgs),
}

#[derive(Subcommand, Debug)]
pub enum TagCmd {
    /// List objects
    #[command(visible_alias = "view")]
    List {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        out: OutArgs,
    },
    /// Add a tag
    #[command(visible_alias = "add")]
    Create {
        /// Tag name
        name: String,
        #[command(flatten)]
        target: Target,
    },
    /// Remove a tag
    Delete {
        /// Tag name
        name: String,
        #[command(flatten)]
        target: Target,
    },
}

#[derive(Args, Debug)]
pub struct ApiArgs {
    /// API path, e.g. projects, experiments/12, v2/experiments
    pub path: String,
    /// HTTP method (default: GET, or POST with --input)
    #[arg(long = "method", short = 'X')]
    pub method: Option<String>,
    /// Add a parameter key=value (item[title]=x nests)
    #[arg(long = "field", short = 'f', value_name = "KEY=VALUE")]
    pub fields: Vec<String>,
    /// Read the request body (JSON) from a file ('-' for standard input)
    #[arg(long, value_name = "FILE")]
    pub input: Option<PathBuf>,
    /// Fetch all pages
    #[arg(long)]
    pub paginate: bool,
    #[command(flatten)]
    pub out: OutArgs,
}

#[derive(Subcommand, Debug)]
pub enum AliasCmd {
    /// Create a shortcut
    Set {
        /// Shortcut name
        name: String,
        /// Command it expands to
        expansion: String,
    },
    /// List shortcuts
    List,
    /// Delete a shortcut
    Delete {
        /// Shortcut name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Print a setting
    Get {
        /// Setting name
        key: String,
    },
    /// Change a setting
    Set {
        /// Setting name
        key: String,
        /// Setting value
        value: String,
    },
    /// List settings
    List,
}

#[derive(Subcommand, Debug)]
pub enum NotebookCmd {
    /// Create a notebook skeleton with an example
    Init {
        /// Notebook directory
        dir: PathBuf,
        /// Labguru project name (default: from lg.toml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Create an empty entry in the right format
    New {
        /// Notebook folder (sub-directory)
        folder: String,
        /// Entry title
        title: String,
        /// Notebook root (default: current directory or a parent with lg.toml)
        #[arg(long, short = 'C')]
        root: Option<PathBuf>,
    },
    /// Show (or with --apply, make) the changes needed in Labguru
    #[command(
        after_help = "Dry run by default. Idempotent: objects are found by external_uuid (lg:<project>/<folder>/<file>), else by title when no other entry owns that object. Experiments are never deleted.

Examples:
  lg notebook push
  lg notebook push --apply --project \"Test project\""
    )]
    Push {
        /// Write to Labguru (default: dry run)
        #[arg(long)]
        apply: bool,
        /// Labguru project name (default: from lg.toml)
        #[arg(long)]
        project: Option<String>,
        /// Notebook root (default: current directory or a parent with lg.toml)
        #[arg(long, short = 'C')]
        root: Option<PathBuf>,
    },
    /// Read Labguru back and report differences with the notebook
    Check {
        /// Labguru project name (default: from lg.toml)
        #[arg(long)]
        project: Option<String>,
        /// Notebook root (default: current directory or a parent with lg.toml)
        #[arg(long, short = 'C')]
        root: Option<PathBuf>,
    },
}
