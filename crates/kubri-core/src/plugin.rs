//! Runs k9s plugins: local commands for the selected object, in a terminal or in the
//! background, with the variables k9s gives them.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::StreamExt;
use futures::channel::mpsc as futures_mpsc;
use futures::stream;
use kube::api::TerminalSize;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;
use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

use crate::k9s::{MAX_INPUTS, Plugin};
use crate::terminal::{Terminal, TerminalInput, TerminalOutput};
use crate::{Error, ObjectRef, ResourceInfo, ResourceRef, Result};

/// What the UI asks to run: a plugin by name, for the selected object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PluginRun {
    pub plugin: String,
    pub target: Option<ObjectRef>,
    /// The view's resource, for a plugin run with nothing selected.
    pub resource: Option<ResourceRef>,
    /// The selected Helm release, in the Helm view.
    pub release: Option<Release>,
    /// For pods: the container `$CONTAINER` names; the default container otherwise.
    pub container: Option<String>,
    /// The list filter, as `$FILTER`.
    pub filter: Option<String>,
    /// The selected row's cells, in column order, for `$COL-<NAME>`.
    pub columns: Vec<Cell>,
    /// Values for the plugin's inputs, by name, as `$INPUT_<NAME>`.
    pub inputs: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub namespace: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Cell {
    pub name: String,
    pub value: String,
}

/// Whether `plugin` is offered for objects of `resource`. Container plugins (`containers`,
/// `co`) run on pods, as Kubri shows containers inside the pod.
pub(crate) fn applies(plugin: &Plugin, resource: &ResourceInfo) -> bool {
    let r = &resource.reference;
    let mut names = vec![
        r.plural.clone(),
        resource.singular.clone(),
        resource.kind.to_lowercase(),
        format!("{}/{}", r.version, r.plural),
    ];
    names.extend(resource.short_names.iter().cloned());
    if !r.group.is_empty() {
        names.push(format!("{}.{}", r.plural, r.group));
        names.push(format!("{}/{}/{}", r.group, r.version, r.plural));
    }
    let pods = is_pods(resource);
    plugin.scopes.iter().any(|scope| {
        let scope = scope.trim().to_lowercase();
        scope == "all"
            || names.contains(&scope)
            || (pods && matches!(scope.as_str(), "containers" | "container" | "co"))
    })
}

/// Whether `plugin` is offered in the Helm view, which k9s also calls `hm`, `chart` and `charts`.
pub(crate) fn applies_to_helm(plugin: &Plugin) -> bool {
    plugin.scopes.iter().any(|scope| {
        matches!(
            scope.trim().to_lowercase().as_str(),
            "all" | "helm" | "hm" | "chart" | "charts"
        )
    })
}

pub(crate) fn is_pods(resource: &ResourceInfo) -> bool {
    resource.reference.group.is_empty() && resource.reference.plural == "pods"
}

/// The variables k9s sets for plugins.
pub(crate) struct Variables(pub Vec<(String, String)>);

impl Variables {
    fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Expands `$NAME`, `${NAME}`, `$COL-READY` and `${COL-%CPU/L}` like k9s: variables
    /// Kubri sets first, then the environment. Unknown ones stay as written, so a shell
    /// script in the arguments keeps its own variables, like `$line` in `read line`.
    pub fn expand(&self, text: &str, columns: &[Cell]) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            rest = &rest[i + 1..];
            let (name, after) = if let Some(braced) = rest.strip_prefix('{') {
                match braced.find('}') {
                    Some(end) => (&braced[..end], &braced[end + 1..]),
                    None => {
                        out.push('$');
                        continue;
                    }
                }
            } else {
                let column = rest.starts_with("COL-");
                let end = rest
                    .char_indices()
                    .skip(if column { 4 } else { 0 })
                    .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
                    .map_or(rest.len(), |(i, _)| i);
                (&rest[..end], &rest[end..])
            };
            if name.is_empty() || name == "COL-" {
                out.push('$');
                continue;
            }
            match self.value(name, columns) {
                Some(value) => out.push_str(&value),
                None => {
                    out.push('$');
                    continue;
                }
            }
            rest = after;
        }
        out.push_str(rest);
        out
    }

    fn value(&self, name: &str, columns: &[Cell]) -> Option<String> {
        if let Some(column) = name.strip_prefix("COL-") {
            // `$COL-0` is the first column, `$COL-NAME` the column of that name. A column the
            // view does not have is empty, as in k9s.
            let cell = match column.parse::<usize>() {
                Ok(i) => columns.get(i),
                Err(_) => columns.iter().find(|c| c.name.eq_ignore_ascii_case(column)),
            };
            return Some(cell.map(|c| c.value.clone()).unwrap_or_default());
        }
        self.get(name)
            .map(str::to_owned)
            .or_else(|| std::env::var(name).ok())
    }
}

/// A plugin ready to start: the program, its expanded arguments and its environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prepared {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub(crate) fn prepare(plugin: &Plugin, variables: &Variables, columns: &[Cell]) -> Result<Prepared> {
    let program = variables.expand(plugin.command.trim(), columns);
    if program.is_empty() {
        return Err(Error::Unsupported(format!(
            "plugin {} has no command",
            plugin.name
        )));
    }
    let mut env = variables.0.clone();
    for column in columns {
        // Environment names cannot hold `=`; such columns are only reachable in arguments.
        if !column.name.is_empty() && !column.name.contains(['=', '\0']) && !column.value.contains('\0') {
            env.push((format!("COL-{}", column.name), column.value.clone()));
        }
    }
    Ok(Prepared {
        program,
        args: plugin.args.iter().map(|a| variables.expand(a, columns)).collect(),
        env,
    })
}

/// Starts a background plugin and does not wait for it, like k9s.
pub(crate) fn spawn_background(prepared: &Prepared) -> Result<()> {
    let mut command = std::process::Command::new(&prepared.program);
    command
        .args(&prepared.args)
        .envs(prepared.env.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(home) = dirs::home_dir() {
        command.current_dir(home);
    }
    let mut child = command.spawn().map_err(|e| spawn_error(&prepared.program, e))?;
    // Reap it when it ends, so it does not linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn spawn_error(program: &str, error: impl std::fmt::Display) -> Error {
    Error::Unsupported(format!("cannot run {program}: {error}"))
}

/// Starts a plugin in a terminal of its own, so interactive commands work.
pub(crate) fn open(prepared: &Prepared, title: String, cols: u16, rows: u16) -> Result<Terminal> {
    let size = |cols: u16, rows: u16| PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = native_pty_system()
        .openpty(size(cols.max(80), rows.max(24)))
        .map_err(|e| spawn_error(&prepared.program, e))?;
    let mut command = CommandBuilder::new(&prepared.program);
    command.args(&prepared.args);
    command.env("TERM", "xterm-256color");
    for (key, value) in &prepared.env {
        command.env(key, value);
    }
    if let Some(home) = dirs::home_dir() {
        command.cwd(home);
    }
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|e| spawn_error(&prepared.program, e))?;
    // The child holds its own end; keeping ours would hide when it exits.
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| spawn_error(&prepared.program, e))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| spawn_error(&prepared.program, e))?;
    let mut killer = child.clone_killer();

    let (tx, rx) = mpsc::channel::<TerminalOutput>(64);
    let reader_tx = tx.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    if reader_tx.blocking_send(TerminalOutput::Data { data }).is_err() {
                        break;
                    }
                }
            }
        }
        let message = match child.wait() {
            Ok(status) if status.success() => None,
            Ok(status) => Some(format!("exited with code {}", status.exit_code())),
            Err(err) => Some(err.to_string()),
        };
        let _ = reader_tx.blocking_send(TerminalOutput::Exit { message });
    });

    // Keystrokes go to a thread, as the terminal's writer blocks.
    let (keys_tx, keys_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        while let Ok(data) = keys_rx.recv() {
            if writer.write_all(&data).and_then(|()| writer.flush()).is_err() {
                break;
            }
        }
    });

    let (resize_tx, mut resize_rx) = futures_mpsc::channel::<TerminalSize>(4);
    let master = pair.master;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                next = resize_rx.next() => match next {
                    Some(s) => {
                        let _ = master.resize(size(s.width, s.height));
                    }
                    None => break,
                },
                // The UI closed the terminal: end the plugin with it.
                () = tx.closed() => {
                    let _ = killer.kill();
                    break;
                }
            }
        }
    });

    let mut input = TerminalInput::new(Some(Box::new(Keys(keys_tx))), Some(resize_tx));
    input.resize(cols, rows);
    Ok(Terminal {
        container: title,
        input,
        // The stream ends with the exit event, though the resize task still holds a sender.
        output: stream::unfold(Some(rx), |rx| async move {
            let mut rx = rx?;
            let event = rx.recv().await?;
            let next = (!matches!(event, TerminalOutput::Exit { .. })).then_some(rx);
            Some((event, next))
        })
        .boxed(),
    })
}

/// Hands keystrokes to the writer thread.
struct Keys(std::sync::mpsc::Sender<Vec<u8>>);

impl AsyncWrite for Keys {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Poll::Ready(
            self.0
                .send(buf.to_vec())
                .map(|()| buf.len())
                .map_err(|_| std::io::ErrorKind::BrokenPipe.into()),
        )
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// The plugin's inputs as `INPUT_<NAME>` variables: the values given, or the defaults, checked
/// against their type the way k9s checks them in its form.
pub(crate) fn input_values(
    plugin: &Plugin,
    given: &BTreeMap<String, String>,
) -> Result<Vec<(String, String)>> {
    let inputs = &plugin.inputs[..plugin.inputs.len().min(MAX_INPUTS)];
    if let Some(unknown) = given.keys().find(|name| !inputs.iter().any(|i| &i.name == *name)) {
        return Err(Error::Unsupported(format!(
            "plugin {} has no input {unknown}",
            plugin.name
        )));
    }
    inputs
        .iter()
        .map(|input| {
            let label = if input.label.is_empty() {
                &input.name
            } else {
                &input.label
            };
            let value = given.get(&input.name).unwrap_or(&input.default).trim().to_owned();
            let invalid = |why: &str| Err(Error::Unsupported(format!("{label} {why}")));
            if value.is_empty() {
                if input.required {
                    return invalid("is required");
                }
            } else {
                match input.kind.as_str() {
                    "number" if value.parse::<f64>().is_err() => return invalid("must be a number"),
                    "bool" if value != "true" && value != "false" => return invalid("must be true or false"),
                    "dropdown" if !input.options.contains(&value) => {
                        return invalid(&format!("must be one of {}", input.options.join(", ")));
                    }
                    _ => {}
                }
            }
            Ok((format!("INPUT_{}", input.name.to_uppercase()), value))
        })
        .collect()
}

/// The variables of a plugin run, for the object it runs on.
pub(crate) fn variables(
    context: &crate::ContextInfo,
    kubeconfig: String,
    run: &PluginRun,
    resource: Option<&ResourceInfo>,
    container: Option<String>,
) -> Variables {
    let target = run.target.as_ref();
    let release = run.release.as_ref().filter(|_| target.is_none());
    let pods = resource.is_some_and(is_pods);
    let mut vars: BTreeMap<&str, String> = BTreeMap::new();
    let name = target
        .map(|t| t.name.clone())
        .or_else(|| release.map(|r| r.name.clone()))
        .unwrap_or_default();
    vars.insert(
        "NAMESPACE",
        target
            .and_then(|t| t.namespace.clone())
            .or_else(|| release.map(|r| r.namespace.clone()))
            .unwrap_or_default(),
    );
    vars.insert("NAME", name.clone());
    vars.insert("CONTAINER", container.unwrap_or_default());
    vars.insert("POD", if pods { name } else { String::new() });
    vars.insert("FILTER", run.filter.clone().unwrap_or_default());
    vars.insert("KUBECONFIG", kubeconfig);
    vars.insert("CLUSTER", context.cluster.clone().unwrap_or_default());
    vars.insert("CONTEXT", context.name.clone());
    vars.insert("USER", context.user.clone().unwrap_or_default());
    vars.insert("GROUPS", String::new());
    let reference = resource.map(|r| &r.reference);
    vars.insert(
        "RESOURCE_GROUP",
        reference.map(|r| r.group.clone()).unwrap_or_default(),
    );
    vars.insert(
        "RESOURCE_VERSION",
        reference.map(|r| r.version.clone()).unwrap_or_default(),
    );
    vars.insert(
        "RESOURCE_NAME",
        match (reference, release) {
            (Some(r), _) => r.plural.clone(),
            // k9s names the Helm view's resource `helm`.
            (None, Some(_)) => "helm".to_owned(),
            (None, None) => String::new(),
        },
    );
    Variables(vars.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(scopes: &[&str], args: &[&str]) -> Plugin {
        Plugin {
            name: "test".into(),
            short_cut: "Ctrl-L".into(),
            description: String::new(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            command: "kubectl".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            background: false,
            confirm: false,
            dangerous: false,
            override_: false,
            inputs: Vec::new(),
        }
    }

    fn resource(group: &str, plural: &str, kind: &str, short: &[&str]) -> ResourceInfo {
        ResourceInfo {
            reference: ResourceRef {
                group: group.into(),
                version: "v1".into(),
                plural: plural.into(),
            },
            kind: kind.into(),
            singular: kind.to_lowercase(),
            short_names: short.iter().map(|s| s.to_string()).collect(),
            namespaced: true,
            verbs: Vec::new(),
            subresources: Vec::new(),
        }
    }

    #[test]
    fn matches_scopes_like_k9s() {
        let pods = resource("", "pods", "Pod", &["po"]);
        let deploys = resource("apps", "deployments", "Deployment", &["deploy"]);
        assert!(applies(&plugin(&["po"], &[]), &pods));
        assert!(applies(&plugin(&["containers"], &[]), &pods));
        assert!(!applies(&plugin(&["containers"], &[]), &deploys));
        assert!(applies(&plugin(&["apps/v1/deployments"], &[]), &deploys));
        assert!(applies(&plugin(&["Deploy"], &[]), &deploys));
        assert!(applies(&plugin(&["all"], &[]), &deploys));
        assert!(!applies(&plugin(&["svc"], &[]), &pods));
    }

    #[test]
    fn expands_variables_and_columns() {
        let vars = Variables(vec![
            ("NAME".into(), "web-1".into()),
            ("NAMESPACE".into(), "shop".into()),
        ]);
        let columns = vec![
            Cell {
                name: "NAME".into(),
                value: "web-1".into(),
            },
            Cell {
                name: "%CPU/L".into(),
                value: "12".into(),
            },
        ];
        let x = |t: &str| vars.expand(t, &columns);
        assert_eq!(x("$NAME -n $NAMESPACE"), "web-1 -n shop");
        assert_eq!(x("${NAME}-debug"), "web-1-debug");
        assert_eq!(x("$COL-NAME/$COL-0"), "web-1/web-1");
        assert_eq!(x("${COL-%CPU/L}%"), "12%");
        assert_eq!(x("$UNSET_FOR_SURE_42 x"), "$UNSET_FOR_SURE_42 x");
        assert_eq!(x("$COL-MISSING."), ".");
        assert_eq!(x("costs $ 5 and ${oops"), "costs $ 5 and ${oops");

        let prepared = prepare(
            &plugin(&["po"], &["logs", "$NAME", "-n", "$NAMESPACE"]),
            &vars,
            &columns,
        )
        .unwrap();
        assert_eq!(prepared.args, ["logs", "web-1", "-n", "shop"]);
        assert!(prepared.env.contains(&("COL-%CPU/L".to_owned(), "12".to_owned())));
    }

    #[test]
    fn sets_the_variables_k9s_sets() {
        let context = crate::ContextInfo {
            name: "prod".into(),
            cluster: Some("eu-1".into()),
            user: Some("admin".into()),
            namespace: None,
            is_default: true,
        };
        let pods = resource("", "pods", "Pod", &["po"]);
        let run = PluginRun {
            target: Some(ObjectRef {
                context: None,
                resource: pods.reference.clone(),
                namespace: Some("shop".into()),
                name: "web-1".into(),
            }),
            filter: Some("web".into()),
            ..PluginRun::default()
        };
        let vars = variables(&context, "/k".into(), &run, Some(&pods), Some("app".into()));
        let get = |vars: &Variables, name: &str| vars.get(name).unwrap_or("?").to_owned();
        for (name, value) in [
            ("NAME", "web-1"),
            ("POD", "web-1"),
            ("NAMESPACE", "shop"),
            ("CONTAINER", "app"),
            ("FILTER", "web"),
            ("CONTEXT", "prod"),
            ("CLUSTER", "eu-1"),
            ("USER", "admin"),
            ("KUBECONFIG", "/k"),
            ("RESOURCE_VERSION", "v1"),
            ("RESOURCE_NAME", "pods"),
            ("RESOURCE_GROUP", ""),
        ] {
            assert_eq!(get(&vars, name), value, "{name}");
        }

        let run = PluginRun {
            release: Some(Release {
                namespace: "shop".into(),
                name: "cart".into(),
            }),
            ..PluginRun::default()
        };
        let vars = variables(&context, "/k".into(), &run, None, None);
        assert_eq!(get(&vars, "NAME"), "cart");
        assert_eq!(get(&vars, "NAMESPACE"), "shop");
        assert_eq!(get(&vars, "POD"), "");
        assert_eq!(get(&vars, "RESOURCE_NAME"), "helm");
        assert!(applies_to_helm(&plugin(&["helm"], &[])));
        assert!(!applies_to_helm(&plugin(&["po"], &[])));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_plugin_in_a_terminal() {
        let vars = Variables(vec![("NAME".into(), "web-1".into())]);
        let mut p = plugin(
            &["po"],
            &["-c", "printf 'hello %s' \"$NAME\"; read line; echo \"got $line\""],
        );
        p.command = "sh".into();
        let prepared = prepare(&p, &vars, &[]).unwrap();
        let mut terminal = open(&prepared, "test".into(), 80, 24).unwrap();
        let mut seen = String::new();
        let mut sent = false;
        let exit = loop {
            let event = tokio::time::timeout(std::time::Duration::from_secs(10), terminal.output.next())
                .await
                .expect("the plugin answers")
                .expect("an exit event ends the stream");
            match event {
                TerminalOutput::Data { data } => {
                    seen.push_str(&String::from_utf8_lossy(&data));
                    if seen.contains("hello web-1") && !sent {
                        terminal.input.write(b"yes\r").await.unwrap();
                        sent = true;
                    }
                }
                TerminalOutput::Exit { message } => break message,
            }
        };
        assert!(seen.contains("got yes"), "{seen:?}");
        assert_eq!(exit, None);
    }

    #[test]
    fn inputs_become_checked_variables() {
        use crate::k9s::PluginInput;
        let input = |name: &str, kind: &str, required: bool, default: &str, options: &[&str]| PluginInput {
            name: name.into(),
            label: String::new(),
            kind: kind.into(),
            required,
            default: default.into(),
            options: options.iter().map(|o| o.to_string()).collect(),
        };
        let mut p = plugin(&["po"], &["--tail", "$INPUT_LINES", "--level=${INPUT_LEVEL}"]);
        p.inputs = vec![
            input("lines", "number", true, "100", &[]),
            input("level", "dropdown", false, "", &["info", "debug"]),
            input("follow", "bool", false, "false", &[]),
        ];
        let given = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let values = input_values(&p, &given(&[("level", "debug")])).unwrap();
        assert_eq!(
            values,
            [
                ("INPUT_LINES".to_owned(), "100".to_owned()),
                ("INPUT_LEVEL".to_owned(), "debug".to_owned()),
                ("INPUT_FOLLOW".to_owned(), "false".to_owned())
            ]
        );
        let prepared = prepare(&p, &Variables(values), &[]).unwrap();
        assert_eq!(prepared.args, ["--tail", "100", "--level=debug"]);
        assert!(
            prepared
                .env
                .contains(&("INPUT_LEVEL".to_owned(), "debug".to_owned()))
        );

        let err = |pairs: &[(&str, &str)]| input_values(&p, &given(pairs)).unwrap_err().to_string();
        assert_eq!(err(&[("lines", " ")]), "lines is required");
        assert_eq!(err(&[("lines", "many")]), "lines must be a number");
        assert_eq!(err(&[("level", "trace")]), "level must be one of info, debug");
        assert_eq!(err(&[("follow", "yes")]), "follow must be true or false");
        assert_eq!(err(&[("color", "red")]), "plugin test has no input color");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reports_a_failing_plugin() {
        let mut p = plugin(&["po"], &["-c", "exit 3"]);
        p.command = "sh".into();
        let prepared = prepare(&p, &Variables(Vec::new()), &[]).unwrap();
        let mut terminal = open(&prepared, "test".into(), 80, 24).unwrap();
        let mut exit = None;
        while let Some(event) = terminal.output.next().await {
            if let TerminalOutput::Exit { message } = event {
                exit = message;
            }
        }
        assert_eq!(exit.as_deref(), Some("exited with code 3"));
        p.command = "kubri-no-such-command".into();
        let prepared = prepare(&p, &Variables(Vec::new()), &[]).unwrap();
        assert!(spawn_background(&prepared).is_err());
    }
}
