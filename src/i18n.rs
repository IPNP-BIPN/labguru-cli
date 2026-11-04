//! English by default, French when LC_ALL (or else LANG) starts with "fr".

use std::sync::OnceLock;

pub fn lang_is_fr(lc_all: Option<&str>, lang: Option<&str>) -> bool {
    lc_all
        .filter(|s| !s.is_empty())
        .or(lang)
        .is_some_and(|s| s.to_ascii_lowercase().starts_with("fr"))
}

pub fn is_fr() -> bool {
    static FR: OnceLock<bool> = OnceLock::new();
    *FR.get_or_init(|| lang_is_fr(std::env::var("LC_ALL").ok().as_deref(), std::env::var("LANG").ok().as_deref()))
}

/// `tr!(english, french)`: picks the string for the system language.
#[macro_export]
macro_rules! tr {
    ($en:expr, $fr:expr $(,)?) => {
        if $crate::i18n::is_fr() { $fr } else { $en }
    };
}

/// French translations of the help texts (command descriptions, argument help).
/// Keys must match the English doc comments exactly; a test checks they all exist.
pub const HELP_FR: &[(&str, &str)] = &[
    (
        "Work with Labguru from the command line",
        "Travailler avec Labguru en ligne de commande",
    ),
    (
        "Labguru host (e.g. my.labguru.com, cle.inserm.fr)",
        "Hôte Labguru (ex. my.labguru.com, cle.inserm.fr)",
    ),
    (
        "Log in, log out, and switch accounts",
        "Se connecter, se déconnecter, changer de compte",
    ),
    ("Log in to a Labguru host", "Se connecter à un hôte Labguru"),
    ("Read a token from standard input", "Lire un token sur l'entrée standard"),
    ("E-mail of the account", "E-mail du compte"),
    (
        "Workspace (account) id, when your login has several",
        "Identifiant du workspace (compte), si votre login en a plusieurs",
    ),
    (
        "Show the hosts and accounts you are logged in to",
        "Afficher les hôtes et comptes connectés",
    ),
    ("Log out of a host or account", "Se déconnecter d'un hôte ou d'un compte"),
    ("Switch the active account or host", "Changer de compte ou d'hôte actif"),
    ("Account to use", "Compte à utiliser"),
    ("Manage projects", "Gérer les projets"),
    ("Manage folders (milestones)", "Gérer les dossiers (milestones)"),
    ("Manage experiments", "Gérer les expériences"),
    (
        "Manage sections of experiments, folders and projects",
        "Gérer les sections des expériences, dossiers et projets",
    ),
    ("Manage notes", "Gérer les notes"),
    ("Manage papers (Key Papers)", "Gérer les articles (Key Papers)"),
    (
        "Upload, download and list attachments",
        "Envoyer, télécharger et lister les pièces jointes",
    ),
    ("Add, remove and list tags", "Ajouter, retirer et lister les tags"),
    ("Open an object in the web browser", "Ouvrir un objet dans le navigateur"),
    (
        "Make an authenticated Labguru API request",
        "Faire une requête authentifiée à l'API Labguru",
    ),
    ("Create command shortcuts", "Créer des raccourcis de commandes"),
    ("Manage lg settings", "Gérer les réglages de lg"),
    (
        "Publish your own notebook (Markdown files) to Labguru",
        "Publier son cahier (fichiers Markdown) dans Labguru",
    ),
    ("Generate shell completion scripts", "Générer l'autocomplétion du shell"),
    ("Generate the man page", "Générer la page de man"),
    ("List objects", "Lister les objets"),
    ("Show one object", "Afficher un objet"),
    ("Create an object", "Créer un objet"),
    ("Edit an object", "Modifier un objet"),
    ("Delete an object", "Supprimer un objet"),
    ("Object id", "Identifiant de l'objet"),
    ("Title", "Titre"),
    ("Description, in Markdown", "Description, en Markdown"),
    ("Start date (YYYY-MM-DD)", "Date de début (AAAA-MM-JJ)"),
    ("End date (YYYY-MM-DD)", "Date de fin (AAAA-MM-JJ)"),
    ("Duration in days, for the Gantt chart", "Durée en jours, pour le Gantt"),
    ("Project id", "Identifiant du projet"),
    ("Folder id", "Identifiant du dossier"),
    ("Experiment id", "Identifiant de l'expérience"),
    ("Note id", "Identifiant de la note"),
    ("Paper id", "Identifiant de l'article"),
    (
        "Parent folder id (default: top level)",
        "Dossier parent (par défaut : premier niveau)",
    ),
    ("Folder status", "Statut du dossier"),
    (
        "Start date (YYYY-MM-DD); Labguru only accepts it at creation",
        "Date de début (AAAA-MM-JJ) ; Labguru ne l'accepte qu'à la création",
    ),
    (
        "Move an experiment to another folder",
        "Déplacer une expérience dans un autre dossier",
    ),
    (
        "Explain why flags (Successful / Failed) cannot be set here",
        "Expliquer pourquoi les flags (Successful / Failed) ne se posent pas ici",
    ),
    (
        "Explain why experiments cannot be signed here",
        "Expliquer pourquoi on ne signe pas d'expérience ici",
    ),
    ("Section name", "Nom de la section"),
    ("Body, in Markdown", "Contenu, en Markdown"),
    (
        "Read the body from a file ('-' for standard input)",
        "Lire le contenu dans un fichier (« - » pour l'entrée standard)",
    ),
    ("Add papers from a BibTeX file", "Ajouter des articles depuis un fichier BibTeX"),
    ("BibTeX file", "Fichier BibTeX"),
    ("Link the papers to this project", "Relier les articles à ce projet"),
    ("Authors", "Auteurs"),
    ("Journal", "Revue"),
    ("Publication date (YYYY-MM-DD)", "Date de publication (AAAA-MM-JJ)"),
    ("Upload a file", "Envoyer un fichier"),
    ("Download a file", "Télécharger un fichier"),
    ("File to upload", "Fichier à envoyer"),
    (
        "Where to write the file (default: its name, '-' for standard output)",
        "Où écrire le fichier (par défaut : son nom, « - » pour la sortie standard)",
    ),
    ("Attachment id", "Identifiant de la pièce jointe"),
    ("Tag name", "Nom du tag"),
    ("Add a tag", "Ajouter un tag"),
    ("Remove a tag", "Retirer un tag"),
    ("Object type", "Type d'objet"),
    (
        "API path, e.g. projects, experiments/12, v2/experiments",
        "Chemin d'API, ex. projects, experiments/12, v2/experiments",
    ),
    (
        "HTTP method (default: GET, or POST with --input)",
        "Méthode HTTP (par défaut : GET, ou POST avec --input)",
    ),
    (
        "Add a parameter key=value (item[title]=x nests)",
        "Ajouter un paramètre clé=valeur (item[title]=x pour imbriquer)",
    ),
    (
        "Read the request body (JSON) from a file ('-' for standard input)",
        "Lire le corps de la requête (JSON) dans un fichier (« - » pour l'entrée standard)",
    ),
    ("Fetch all pages", "Récupérer toutes les pages"),
    ("Create a shortcut", "Créer un raccourci"),
    ("List shortcuts", "Lister les raccourcis"),
    ("Delete a shortcut", "Supprimer un raccourci"),
    ("Shortcut name", "Nom du raccourci"),
    ("Command it expands to", "Commande qu'il remplace"),
    ("Print a setting", "Afficher un réglage"),
    ("Change a setting", "Changer un réglage"),
    ("List settings", "Lister les réglages"),
    ("Setting name", "Nom du réglage"),
    ("Setting value", "Valeur du réglage"),
    (
        "Create a notebook skeleton with an example",
        "Créer la structure d'un cahier, avec un exemple",
    ),
    ("Create an empty entry in the right format", "Créer une entrée vide au bon format"),
    (
        "Show (or with --apply, make) the changes needed in Labguru",
        "Afficher (ou avec --apply, faire) les changements à apporter dans Labguru",
    ),
    (
        "Read Labguru back and report differences with the notebook",
        "Relire Labguru et signaler les écarts avec le cahier",
    ),
    ("Notebook directory", "Dossier du cahier"),
    ("Notebook folder (sub-directory)", "Dossier du cahier (sous-répertoire)"),
    ("Entry title", "Titre de l'entrée"),
    ("Write to Labguru (default: dry run)", "Écrire dans Labguru (par défaut : à blanc)"),
    (
        "Labguru project name (default: from lg.toml)",
        "Nom du projet Labguru (par défaut : celui de lg.toml)",
    ),
    (
        "Notebook root (default: current directory or a parent with lg.toml)",
        "Racine du cahier (par défaut : dossier courant ou un parent avec lg.toml)",
    ),
    ("Shell", "Shell"),
    (
        "Output JSON with the given fields (comma-separated); without fields, list them",
        "Sortie JSON avec ces champs (séparés par des virgules) ; sans champ, les liste",
    ),
    (
        "Filter JSON output with a jq expression",
        "Filtrer la sortie JSON avec une expression jq",
    ),
    (
        "Format JSON output with a template: {{.field}}",
        "Mettre en forme la sortie JSON avec un modèle : {{.champ}}",
    ),
    ("Open in the web browser", "Ouvrir dans le navigateur"),
    ("Maximum number of results", "Nombre maximal de résultats"),
    ("Print help", "Afficher l'aide"),
    ("Print version", "Afficher la version"),
    (
        "Print this message or the help of the given subcommand(s)",
        "Afficher ce message ou l'aide des sous-commandes indiquées",
    ),
    ("DOI (e.g. 10.1038/171737a0)", "DOI (ex. 10.1038/171737a0)"),
    ("Web address of the paper", "Adresse web de l'article"),
    ("Skip the confirmation prompt", "Ne pas demander confirmation"),
    (
        "Directory where to write lg.1 (default: standard output)",
        "Dossier où écrire lg.1 (par défaut : sortie standard)",
    ),
];

/// Paragraphs of `after_help` (before "Examples:").
pub const PROSE_FR: &[(&str, &str)] = &[
    (
        "The token is added for you: in the query for GET, in the JSON body otherwise.\nPaths without a version use /api/v1 (and .json).",
        "Le token est ajouté pour vous : dans la requête pour GET, dans le corps JSON sinon.\nLes chemins sans version utilisent /api/v1 (et .json).",
    ),
    (
        "Without --with-token, lg asks for your e-mail and password once and stores only the token.",
        "Sans --with-token, lg demande une fois votre e-mail et votre mot de passe et ne garde que le token.",
    ),
    (
        "Dry run by default. Idempotent: objects are found by external_uuid (lg:<project>/<folder>/<file>), else by title when no other entry owns that object. Experiments are never deleted.",
        "À blanc par défaut. Idempotent : les objets sont retrouvés par external_uuid (lg:<projet>/<dossier>/<fichier>), sinon par titre si aucune autre entrée ne possède cet objet. Aucune expérience n'est jamais supprimée.",
    ),
];

/// Translates help texts and the "Examples:" headers when the system language is French.
pub fn localize(cmd: clap::Command) -> clap::Command {
    if !is_fr() {
        return cmd;
    }
    let mut cmd = cmd;
    cmd.build(); // creates --help / --version so that they can be translated too
    localize_with(cmd)
}

fn fr(s: &str) -> Option<&'static str> {
    HELP_FR.iter().find(|(en, _)| *en == s).map(|(_, f)| *f)
}

pub fn localize_with(mut cmd: clap::Command) -> clap::Command {
    cmd = cmd
        .subcommand_help_heading("Commandes")
        .help_template("{before-help}{about-with-newline}\nUtilisation : {usage}\n\n{all-args}{after-help}");
    if let Some(a) = cmd.get_about().and_then(|a| fr(&a.to_string())) {
        cmd = cmd.about(a);
    }
    if let Some(h) = cmd.get_after_help().map(|h| h.to_string()) {
        let h = PROSE_FR.iter().fold(h, |h, (en, f)| h.replace(en, f));
        cmd = cmd.after_help(h.replace("Examples:", "Exemples :"));
    }
    cmd = cmd.mut_args(|a| match a.get_help().and_then(|h| fr(&h.to_string())) {
        Some(f) => a.help(f),
        None => a,
    });
    let names: Vec<String> = cmd.get_subcommands().map(|s| s.get_name().to_string()).collect();
    for n in names {
        cmd = cmd.mut_subcommand(n, localize_with);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_detection() {
        assert!(lang_is_fr(Some("fr_FR.UTF-8"), Some("en_US")));
        assert!(!lang_is_fr(Some("en_US.UTF-8"), Some("fr_FR")));
        assert!(lang_is_fr(Some(""), Some("fr_CA")));
        assert!(!lang_is_fr(None, None));
    }
}
