//! Kubri on the command line: `kubri lint` runs the sanitizer, for CI, and `kubri snapshot`
//! saves a snapshot file, both without a window. The desktop app (`kubri`) and the web server
//! (`kubri-server`) both understand these commands.

mod report;

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use kubri_core::{Kubri, Level};

pub use report::{sarif, text};

/// The commands, as the first argument: `kubri lint …`.
pub const COMMANDS: [&str; 2] = ["lint", "snapshot"];

/// True when the arguments start with one of [`COMMANDS`], so the program runs it instead of
/// its window or server.
pub fn requested(args: &[OsString]) -> bool {
    args.get(1)
        .and_then(|a| a.to_str())
        .is_some_and(|a| COMMANDS.contains(&a))
}

/// A snapshot file given as the only argument, as when the system opens a `.kubri` file with
/// Kubri, or `kubri open FILE`.
pub fn file_to_open(args: &[OsString]) -> Option<PathBuf> {
    match args {
        [_, file] if Path::new(file).extension().is_some_and(|e| e == "kubri") => Some(file.into()),
        [_, open, file] if open == "open" => Some(file.into()),
        _ => None,
    }
}

#[derive(Parser)]
#[command(about = "Kubri on the command line", disable_version_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a namespace or the whole cluster for problems and risky settings, and exit with 1
    /// when there are any of the level given with --fail-on.
    Lint(LintArgs),
    /// Save a namespace or the whole cluster to a snapshot file that opens in Kubri anywhere,
    /// without access to the cluster. Secret values are left out.
    Snapshot(SnapshotArgs),
}

#[derive(Args)]
struct Target {
    /// The kubeconfig context; the current one when left out.
    #[arg(long)]
    context: Option<String>,
    /// The namespace; the context's namespace when left out.
    #[arg(short, long, conflicts_with = "all_namespaces")]
    namespace: Option<String>,
    /// The whole cluster, with nodes, volumes and cluster roles.
    #[arg(short = 'A', long, alias = "all")]
    all_namespaces: bool,
}

#[derive(Args)]
struct LintArgs {
    #[command(flatten)]
    target: Target,
    /// How to write the report.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// Write the report to this file instead of the standard output.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Exit with 1 when an object has a problem of this level or worse.
    #[arg(long, value_enum, default_value_t = FailOn::Error)]
    fail_on: FailOn,
    /// Exit with 1 when the score is below this, from 0 to 100.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    min_score: Option<u8>,
}

#[derive(Args)]
struct SnapshotArgs {
    #[command(flatten)]
    target: Target,
    /// The file to write, or a folder for a file named after the context and time.
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// Readable, grouped by kind.
    Text,
    /// The whole report as JSON.
    Json,
    /// SARIF 2.1.0, for GitHub code scanning and other tools.
    Sarif,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FailOn {
    Error,
    Warning,
    Never,
}

/// Runs the command in `args` (the program's arguments, the program first) and returns the
/// exit code: 0 when it went well, 1 when lint found what --fail-on names, 2 when
/// it could not run.
pub fn run(args: Vec<OsString>) -> u8 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return if err.use_stderr() { 2 } else { 0 };
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => return fail(&err.to_string()),
    };
    runtime.block_on(async {
        let kubri = Kubri::load();
        // Nothing these commands do changes a cluster; make sure of it.
        kubri.set_read_only();
        match cli.command {
            Command::Lint(args) => lint(&kubri, args).await,
            Command::Snapshot(args) => snapshot(&kubri, args).await,
        }
    })
}

fn fail(message: &str) -> u8 {
    eprintln!("kubri: {message}");
    2
}

/// Connects to the target's context and returns the namespace to use, `None` for all.
async fn connect(kubri: &Kubri, target: &Target) -> Result<(String, Option<String>), String> {
    let context = match &target.context {
        Some(context) => context.clone(),
        None => {
            let list = kubri.contexts().await.map_err(|e| e.to_string())?;
            list.contexts
                .iter()
                .find(|c| c.is_default)
                .map(|c| c.name.clone())
                .ok_or("the kubeconfig has no current context, pass --context")?
        }
    };
    let cluster = kubri.switch_context(&context).await.map_err(|e| e.to_string())?;
    let namespace = if target.all_namespaces {
        None
    } else {
        Some(target.namespace.clone().unwrap_or(cluster.default_namespace))
    };
    Ok((context, namespace))
}

async fn lint(kubri: &Kubri, args: LintArgs) -> u8 {
    let (context, namespace) = match connect(kubri, &args.target).await {
        Ok(target) => target,
        Err(err) => return fail(&err),
    };
    let report = match kubri.sanitize(namespace.as_deref()).await {
        Ok(report) => report,
        Err(err) => return fail(&err.to_string()),
    };
    let out = match args.format {
        Format::Text => text(&report, &context),
        Format::Json => {
            let json = serde_json::json!({ "context": context, "report": report });
            format!("{}\n", serde_json::to_string_pretty(&json).unwrap_or_default())
        }
        Format::Sarif => format!(
            "{}\n",
            serde_json::to_string_pretty(&sarif(&report, &context)).unwrap_or_default()
        ),
    };
    let written = match &args.output {
        Some(path) => std::fs::write(path, out.as_bytes()),
        None => std::io::stdout().write_all(out.as_bytes()),
    };
    if let Err(err) = written {
        return fail(&format!("cannot write the report: {err}"));
    }

    let worst = report
        .sections
        .iter()
        .flat_map(|s| &s.findings)
        .map(|f| f.level)
        .max();
    let too_bad = match args.fail_on {
        FailOn::Error => worst == Some(Level::Error),
        FailOn::Warning => worst >= Some(Level::Warning),
        FailOn::Never => false,
    };
    let too_low = args.min_score.is_some_and(|min| report.score < min);
    if too_low {
        eprintln!(
            "kubri: the score {} is below --min-score {}",
            report.score,
            args.min_score.unwrap_or_default()
        );
    }
    if too_bad || too_low { 1 } else { 0 }
}

async fn snapshot(kubri: &Kubri, args: SnapshotArgs) -> u8 {
    let namespace = match connect(kubri, &args.target).await {
        Ok((_, namespace)) => namespace,
        Err(err) => return fail(&err),
    };
    let (name, bytes) = match kubri.take_snapshot(namespace.as_deref()).await {
        Ok(snapshot) => snapshot,
        Err(err) => return fail(&err.to_string()),
    };
    let path = match args.output {
        Some(path) if path.is_dir() => path.join(name),
        Some(path) => path,
        None => PathBuf::from(name),
    };
    if let Err(err) = std::fs::write(&path, bytes) {
        return fail(&format!("cannot write {}: {err}", path.display()));
    }
    println!("{}", path.display());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn knows_its_commands() {
        assert!(requested(&args(&["kubri", "lint", "-A"])));
        assert!(requested(&args(&["kubri-server", "snapshot"])));
        assert!(!requested(&args(&["kubri"])));
        assert!(!requested(&args(&["kubri", "--mcp"])));
        assert!(!requested(&args(&["kubri-server", "--addr", "lint"])));
    }

    #[test]
    fn finds_a_snapshot_to_open() {
        assert_eq!(
            file_to_open(&args(&["kubri", "/tmp/prod.kubri"])),
            Some(PathBuf::from("/tmp/prod.kubri"))
        );
        assert_eq!(
            file_to_open(&args(&["kubri", "open", "saved"])),
            Some(PathBuf::from("saved"))
        );
        assert_eq!(file_to_open(&args(&["kubri", "notes.txt"])), None);
        assert_eq!(file_to_open(&args(&["kubri"])), None);
    }

    #[test]
    fn parses_lint_options() {
        let cli = Cli::try_parse_from(args(&[
            "kubri",
            "lint",
            "-n",
            "shop",
            "--format",
            "sarif",
            "--fail-on",
            "warning",
        ]))
        .unwrap();
        let Command::Lint(lint) = cli.command else {
            panic!("not lint")
        };
        assert_eq!(lint.target.namespace.as_deref(), Some("shop"));
        assert!(matches!(lint.format, Format::Sarif));
        assert!(lint.fail_on == FailOn::Warning);
        assert!(Cli::try_parse_from(args(&["kubri", "lint", "-n", "a", "-A"])).is_err());
        assert!(Cli::try_parse_from(args(&["kubri", "lint", "--min-score", "101"])).is_err());
    }
}
