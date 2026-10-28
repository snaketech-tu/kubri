//! Apps opened from the Dock, Finder or a desktop launcher do not get what the shell profile
//! sets up: the `PATH` to Homebrew's `helm` and `kubectl`, to credential plugins such as
//! `aws` or `gke-gcloud-auth-plugin`, and to the commands k9s plugins run, or `KUBECONFIG`.
//! Like VS Code, Kubri asks the user's login shell for its environment at startup.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Set when Kubri runs itself inside the shell to print the environment.
const PRINT: &str = "KUBRI_PRINT_ENV";
const MARKER: &str = "__KUBRI_ENV__";
/// Variables that describe the shell rather than the user's setup.
const SKIP: &[&str] = &["_", "SHLVL", "PWD", "OLDPWD", "TERM", PRINT];

/// Whether this process was started only to print its environment.
pub fn print_requested() -> bool {
    std::env::var_os(PRINT).is_some()
}

pub fn print() {
    let vars: HashMap<String, String> = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect();
    print!(
        "{MARKER}{}{MARKER}",
        serde_json::to_string(&vars).unwrap_or_default()
    );
}

/// Takes over the login shell's environment, unless Kubri was started from a terminal,
/// which already has it. Must run before any other thread starts.
pub fn load() {
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let exe = match std::env::current_exe() {
        Ok(exe) => exe.to_string_lossy().into_owned(),
        Err(err) => {
            tracing::warn!("cannot read the shell environment: {err}");
            return;
        }
    };
    let quoted = format!("'{}'", exe.replace('\'', r"'\''"));
    // Nushell runs a quoted path only with `^` in front.
    let command = if shell.ends_with("/nu") {
        format!("^{quoted}")
    } else {
        quoted
    };
    match read(&shell, &command, Duration::from_secs(5)) {
        Ok(vars) => {
            for (key, value) in vars {
                if SKIP.contains(&key.as_str()) || std::env::var(&key).is_ok_and(|v| v == value) {
                    continue;
                }
                // SAFETY: called at startup before any other thread exists, so nothing reads
                // the environment at the same time.
                unsafe { std::env::set_var(&key, &value) };
            }
        }
        Err(err) => tracing::warn!(shell, "cannot read the shell environment: {err}"),
    }
}

/// Runs `command` in an interactive login shell, which prints the environment between markers.
fn read(shell: &str, command: &str, timeout: Duration) -> Result<HashMap<String, String>, String> {
    let mut child = Command::new(shell)
        .args(["-i", "-l", "-c", command])
        .env(PRINT, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        let _ = stdout.read_to_string(&mut output);
        output
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "the shell took longer than {} seconds",
                    timeout.as_secs()
                ));
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    let output = reader.join().unwrap_or_default();
    parse(&output).ok_or_else(|| "the shell printed no environment".into())
}

/// The environment between the markers; profiles may print greetings around it.
fn parse(output: &str) -> Option<HashMap<String, String>> {
    let (_, rest) = output.split_once(MARKER)?;
    let (json, _) = rest.split_once(MARKER)?;
    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_environment_among_greetings() {
        let output =
            format!("Welcome back!\n{MARKER}{{\"PATH\":\"/opt/homebrew/bin:/usr/bin\"}}{MARKER}\nbye");
        assert_eq!(parse(&output).unwrap()["PATH"], "/opt/homebrew/bin:/usr/bin");
        assert!(parse("no markers").is_none());
        assert!(parse(&format!("{MARKER}not json{MARKER}")).is_none());
    }

    #[test]
    fn reads_what_the_shell_prints() {
        let command =
            format!(r#"echo hello; printf '%s' '{MARKER}{{"FROM_SHELL":"'"$KUBRI_PRINT_ENV"'"}}{MARKER}'"#);
        let vars = read("/bin/sh", &command, Duration::from_secs(5)).unwrap();
        assert_eq!(vars["FROM_SHELL"], "1");
    }

    #[test]
    fn gives_up_on_a_hanging_shell() {
        let err = read("/bin/sh", "sleep 5", Duration::from_millis(200)).unwrap_err();
        assert!(err.contains("longer than"));
    }
}
