//! Explain: sends what Kubri knows about a pod, an event, log lines, a finding or a failed action
//! to a model the user chose, with the user's own key, and returns the model's explanation.
//!
//! Nothing is sent until the user has picked a provider and looked at the prompt: the UI first
//! asks for [`ExplainPrompt`], shows it, and only then sends its text. Secrets, tokens and
//! secret-looking values are hidden on the way (see [`crate::redact`]).

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, Kubri, ObjectRef, ResourceRef, Result, redact};

/// The model Explain uses with Anthropic when the settings name none.
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-opus-5-5";
const ANTHROPIC_URL: &str = "https://api.anthropic.com";
const OPENAI_URL: &str = "https://api.openai.com/v1";

/// Room for the model's thinking and its answer.
const MAX_TOKENS: u32 = 16_000;
/// How long a model may take to answer.
const TIMEOUT: Duration = Duration::from_secs(300);
/// The longest prompt Explain sends; longer sections are cut.
const MAX_PROMPT: usize = 80_000;
/// The longest single section, such as an object's YAML.
const MAX_SECTION: usize = 24_000;
/// Log lines sent from a selection, counted from the end.
const MAX_LOG_LINES: usize = 300;

/// What the model is told about its job.
pub const SYSTEM_PROMPT: &str = "You help a person who operates Kubernetes clusters with Kubri, a \
Kubernetes interface. They show you an object, its events, log lines, a finding or a failed action. \
Explain in plain words what it shows: the most likely cause first, then how to confirm it and how to \
fix it, with kubectl commands where they help. Keep it short and use Markdown sparingly: short \
paragraphs, lists and code blocks; headings only for long answers. Values shown as <hidden> were \
removed before sending on purpose; do not ask for them. If the data is not enough to tell, say what \
to look at next.";

/// Where Explain sends its questions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Explain is off: nothing is sent anywhere.
    #[default]
    None,
    /// The Anthropic Messages API.
    Anthropic,
    /// Any endpoint that speaks OpenAI's chat completions, such as Ollama or LM Studio.
    #[serde(rename = "openai")]
    OpenAi,
}

/// The model Explain asks and how to reach it. The key is kept apart, never in the settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExplainSettings {
    pub provider: Provider,
    /// Empty for the provider's own address.
    pub base_url: String,
    /// Empty for the default model, where the provider has one.
    pub model: String,
}

impl ExplainSettings {
    pub(crate) fn normalized(mut self) -> Result<Self> {
        self.base_url = self.base_url.trim().trim_end_matches('/').to_owned();
        self.model = self.model.trim().to_owned();
        if !self.base_url.is_empty()
            && !self.base_url.starts_with("https://")
            && !self.base_url.starts_with("http://")
        {
            return Err(Error::Unsupported(format!(
                "the address `{}` does not start with https:// or http://",
                self.base_url
            )));
        }
        Ok(self)
    }

    /// The model asked, or `None` when the settings name none and the provider has no default.
    pub fn model(&self) -> Option<String> {
        match (self.provider, self.model.as_str()) {
            (_, model) if !model.is_empty() => Some(model.to_owned()),
            (Provider::Anthropic, _) => Some(DEFAULT_ANTHROPIC_MODEL.to_owned()),
            _ => None,
        }
    }

    /// The address questions go to, or `None` while Explain is off.
    pub fn endpoint(&self) -> Option<String> {
        let base = |default: &str| {
            if self.base_url.is_empty() {
                default.to_owned()
            } else {
                self.base_url.clone()
            }
        };
        match self.provider {
            Provider::None => None,
            Provider::Anthropic => Some(format!("{}/v1/messages", base(ANTHROPIC_URL))),
            Provider::OpenAi => Some(format!("{}/chat/completions", base(OPENAI_URL))),
        }
    }
}

/// What to explain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExplainSubject {
    /// A pod, an event or any other object, with what Kubri knows about it.
    #[serde(rename_all = "camelCase")]
    Object { target: ObjectRef },
    /// Log lines the user picked, of a pod or a workload.
    #[serde(rename_all = "camelCase")]
    Logs {
        target: ObjectRef,
        #[serde(default)]
        container: Option<String>,
        lines: String,
    },
    /// Issues the sanitizer found on an object.
    #[serde(rename_all = "camelCase")]
    Finding { target: ObjectRef, issues: Vec<String> },
    /// An action that failed, with the error it ended with.
    #[serde(rename_all = "camelCase")]
    Failure {
        action: String,
        #[serde(default)]
        target: Option<ObjectRef>,
        error: String,
    },
}

/// Exactly what Explain would send, for the user to look at first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPrompt {
    /// A short name for what is explained, such as `Pod shop/web-1`.
    pub title: String,
    pub system: String,
    pub text: String,
    pub provider: Provider,
    /// `None` while Explain is off or no model is named.
    pub model: Option<String>,
    pub endpoint: Option<String>,
    /// Whether a key is stored. Local models often need none.
    pub has_key: bool,
}

/// The model's explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainAnswer {
    /// Markdown.
    pub text: String,
    /// The model that answered, as the provider names it.
    pub model: String,
    /// The answer ended at the length limit.
    pub cut_short: bool,
}

/// A part of the prompt, with a heading.
struct Section {
    title: String,
    body: String,
}

impl Section {
    fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
        }
    }
}

/// Cuts `text` to at most `max` bytes at a line break, saying so.
fn cut(text: &str, max: usize, from_end: bool) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    if from_end {
        let mut start = text.len() - max;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        let start = text[start..].find('\n').map_or(start, |i| start + i + 1);
        format!("… (earlier lines left out)\n{}", &text[start..])
    } else {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let end = text[..end].rfind('\n').unwrap_or(end);
        format!("{}\n… (the rest is left out)", &text[..end])
    }
}

/// The prompt's text: each section under a heading, then the question.
fn compose(sections: &[Section], question: &str) -> String {
    let mut out = String::new();
    for s in sections {
        let body = cut(s.body.trim_end(), MAX_SECTION, false);
        out.push_str(&format!("## {}\n\n{body}\n\n", s.title));
    }
    out.push_str(question);
    redact::text(&cut(&out, MAX_PROMPT, false))
}

fn yaml(value: &Value) -> String {
    serde_saphyr::to_string(value).unwrap_or_else(|e| format!("cannot write YAML: {e}"))
}

fn place(target: &ObjectRef) -> String {
    match &target.namespace {
        Some(ns) => format!("{ns}/{}", target.name),
        None => target.name.clone(),
    }
}

fn is_pod(resource: &ResourceRef) -> bool {
    resource.group.is_empty() && resource.plural == "pods"
}

fn kind_of(value: &Value) -> String {
    value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("Object")
        .to_owned()
}

impl Kubri {
    pub fn set_explain_settings(&self, value: ExplainSettings) -> Result<crate::Settings> {
        let value = value.normalized()?;
        self.update_settings(|s| {
            s.explain = value;
            Ok(())
        })
    }

    /// The object as YAML without secrets, with what else Kubri knows: for a pod the likely
    /// problem, warning events and logs of failing containers, for others their events.
    async fn object_sections(&self, target: &ObjectRef, value: &Value, out: &mut Vec<Section>) {
        let kind = kind_of(value);
        // For a pod, what Kubri found comes first: it is what the person reads first too.
        if is_pod(&target.resource) {
            let report = self.pod_report(target, value).await;
            out.push(Section::new("What Kubri found", report));
        }
        let mut shown = value.clone();
        redact::for_explain(&mut shown);
        out.push(Section::new(
            format!("{kind} {}", place(target)),
            format!("```yaml\n{}```", yaml(&shown)),
        ));
        if is_pod(&target.resource) {
            return;
        }
        if let Ok(events) = self.event_lines(target).await
            && !events.is_empty()
        {
            let skip = events.len().saturating_sub(20);
            out.push(Section::new("Events", events[skip..].join("\n")));
        }
    }

    /// The object an Event is about, found by its kind and API version.
    async fn involved(&self, event: &Value) -> Option<ObjectRef> {
        let involved = event.get("involvedObject").or_else(|| event.get("regarding"))?;
        let field = |name: &str| involved.get(name).and_then(Value::as_str);
        let (kind, name) = (field("kind")?, field("name")?);
        let api_version = field("apiVersion").unwrap_or("v1");
        let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
        let resources = self.resources().await.ok()?;
        let info = resources
            .iter()
            .find(|r| r.reference.group == group && r.reference.version == version && r.kind == kind)
            .or_else(|| {
                resources
                    .iter()
                    .find(|r| r.reference.group == group && r.kind == kind)
            })?;
        Some(ObjectRef {
            context: None,
            resource: info.reference.clone(),
            namespace: field("namespace").filter(|_| info.namespaced).map(Into::into),
            name: name.to_owned(),
        })
    }

    /// Gathers what Explain would send about `subject`. Nothing leaves Kubri here.
    pub async fn explain_prompt(&self, subject: &ExplainSubject) -> Result<ExplainPrompt> {
        let mut sections = Vec::new();
        let (title, question) = match subject {
            ExplainSubject::Object { target } => {
                let value = self.get(target).await?;
                let kind = kind_of(&value);
                if kind == "Event" {
                    let mut event = value.clone();
                    redact::noise(&mut event);
                    sections.push(Section::new("Event", format!("```yaml\n{}```", yaml(&event))));
                    if let Some(about) = self.involved(&value).await
                        && let Ok(object) = self.get(&about).await
                    {
                        self.object_sections(&about, &object, &mut sections).await;
                    }
                    let reason = value.get("reason").and_then(Value::as_str).unwrap_or("this");
                    (
                        format!("Event {reason}"),
                        "What does this event mean, is something wrong, and how do I fix it?",
                    )
                } else {
                    self.object_sections(target, &value, &mut sections).await;
                    (
                        format!("{kind} {}", place(target)),
                        if is_pod(&target.resource) {
                            "What is the state of this pod? If something is wrong, why, and how do I fix it?"
                        } else {
                            "What is the state of this object? Point out anything that looks wrong, and how to fix it."
                        },
                    )
                }
            }
            ExplainSubject::Logs {
                target,
                container,
                lines,
            } => {
                if is_pod(&target.resource)
                    && let Ok(pod) = self.get(target).await
                {
                    let mut status = self.pod_report(target, &pod).await;
                    // The report's own log lines would repeat the ones picked.
                    if let Some(i) = status.find("\n\nLast log lines of ") {
                        status.truncate(i);
                    }
                    sections.push(Section::new(format!("Pod {}", place(target)), status));
                }
                let lines: Vec<&str> = lines.lines().collect();
                let skip = lines.len().saturating_sub(MAX_LOG_LINES);
                let from = match container {
                    Some(c) => format!("Log lines of {} (container {c})", place(target)),
                    None => format!("Log lines of {}", place(target)),
                };
                sections.push(Section::new(
                    from,
                    format!(
                        "```\n{}{}\n```",
                        if skip > 0 {
                            "… (earlier lines left out)\n"
                        } else {
                            ""
                        },
                        lines[skip..].join("\n")
                    ),
                ));
                (
                    format!("Logs of {}", target.name),
                    "What do these log lines say? If they show a problem, what is the likely cause and how do I fix it?",
                )
            }
            ExplainSubject::Finding { target, issues } => {
                sections.push(Section::new(
                    "What Kubri's sanitizer found",
                    issues
                        .iter()
                        .map(|i| format!("- {i}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ));
                if let Ok(value) = self.get(target).await {
                    self.object_sections(target, &value, &mut sections).await;
                }
                (
                    format!("Findings on {}", target.name),
                    "Why do these findings matter, and how do I fix them? Show the change to make.",
                )
            }
            ExplainSubject::Failure {
                action,
                target,
                error,
            } => {
                let on = target
                    .as_ref()
                    .map(|t| format!(" on {}", place(t)))
                    .unwrap_or_default();
                sections.push(Section::new(
                    "What failed",
                    format!("The action \"{action}\"{on} failed with:\n\n```\n{error}\n```"),
                ));
                if let Some(target) = target
                    && let Ok(value) = self.get(target).await
                {
                    self.object_sections(target, &value, &mut sections).await;
                }
                (
                    format!("{action} failed"),
                    "Why did this fail, and what can I do about it?",
                )
            }
        };
        let settings = self.settings().explain;
        Ok(ExplainPrompt {
            title,
            system: SYSTEM_PROMPT.to_owned(),
            text: compose(&sections, question),
            provider: settings.provider,
            model: settings.model(),
            endpoint: settings.endpoint(),
            has_key: false,
        })
    }

    /// Sends `text`, as the user saw it, to the model in the settings, with `key` if any.
    pub async fn explain(&self, text: &str, key: Option<&str>) -> Result<ExplainAnswer> {
        ask(&self.settings().explain, key, text).await
    }
}

static CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> = LazyLock::new(|| {
    // kube brings ring; reqwest uses whatever provider the process has.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
});

fn model_error(message: impl Into<String>) -> Error {
    Error::Model(message.into())
}

/// The message in an error answer, in the shapes Anthropic and OpenAI use.
fn error_message(status: u16, body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error"))
                .or_else(|| v.get("message"))
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status {
        401 | 403 => format!("the provider refused the API key ({status}): {message}"),
        404 => format!("not found ({status}), check the address and model: {message}"),
        429 => format!("too many requests or no credit left ({status}): {message}"),
        _ => format!("the provider answered {status}: {message}"),
    }
}

/// Asks the model in `settings`. The text is redacted once more, in case it was not shown first.
pub async fn ask(settings: &ExplainSettings, key: Option<&str>, text: &str) -> Result<ExplainAnswer> {
    let endpoint = settings
        .endpoint()
        .ok_or_else(|| Error::Unsupported("Explain is off: pick a provider in the settings first".into()))?;
    let model = settings
        .model()
        .ok_or_else(|| Error::Unsupported("Explain needs a model name in the settings".into()))?;
    let key = key.map(str::trim).filter(|k| !k.is_empty());
    let text = redact::text(text);
    let client = CLIENT.as_ref().map_err(|e| model_error(e.clone()))?;
    let request = match settings.provider {
        Provider::Anthropic => {
            let key = key.ok_or_else(|| {
                Error::Unsupported("Explain needs an Anthropic API key in the settings".into())
            })?;
            client
                .post(&endpoint)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({
                    "model": model,
                    "max_tokens": MAX_TOKENS,
                    "system": SYSTEM_PROMPT,
                    "messages": [{ "role": "user", "content": text }],
                }))
        }
        Provider::OpenAi => {
            let request = client.post(&endpoint).json(&json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": SYSTEM_PROMPT },
                    { "role": "user", "content": text },
                ],
            }));
            match key {
                Some(key) => request.bearer_auth(key),
                None => request,
            }
        }
        Provider::None => unreachable!("no endpoint while Explain is off"),
    };
    let response = request.send().await.map_err(|e| {
        let mut message = e.to_string();
        let mut source = std::error::Error::source(&e);
        while let Some(cause) = source {
            message.push_str(&format!(": {cause}"));
            source = cause.source();
        }
        model_error(format!("cannot reach {endpoint}: {message}"))
    })?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| model_error(format!("the answer broke off: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(model_error(error_message(status, &body)));
    }
    let value: Value =
        serde_json::from_str(&body).map_err(|e| model_error(format!("the answer is not JSON: {e}")))?;
    match settings.provider {
        Provider::Anthropic => anthropic_answer(&value, &model),
        _ => openai_answer(&value, &model),
    }
}

fn anthropic_answer(value: &Value, asked: &str) -> Result<ExplainAnswer> {
    let stop = value.get("stop_reason").and_then(Value::as_str).unwrap_or("");
    if stop == "refusal" {
        return Err(model_error("the model declined to answer this"));
    }
    let text: Vec<&str> = value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    let text = text.join("").trim().to_owned();
    if text.is_empty() {
        return Err(model_error(format!(
            "the model gave no answer (stop reason `{stop}`)"
        )));
    }
    Ok(ExplainAnswer {
        text,
        model: value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(asked)
            .to_owned(),
        cut_short: stop == "max_tokens",
    })
}

fn openai_answer(value: &Value, asked: &str) -> Result<ExplainAnswer> {
    let choice = value.pointer("/choices/0");
    let text = choice
        .and_then(|c| c.pointer("/message/content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let finish = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if text.is_empty() {
        let refusal = choice
            .and_then(|c| c.pointer("/message/refusal"))
            .and_then(Value::as_str);
        return Err(model_error(match refusal {
            Some(r) => format!("the model declined to answer: {r}"),
            None => format!("the model gave no answer (finish reason `{finish}`)"),
        }));
    }
    Ok(ExplainAnswer {
        text,
        model: value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(asked)
            .to_owned(),
        cut_short: finish == "length",
    })
}

/// Where the API key is kept when no keychain holds it: `explain-key` in Kubri's config folder,
/// readable only by its owner.
pub fn key_path() -> Option<PathBuf> {
    Some(crate::settings::dir()?.join("explain-key"))
}

/// The stored key, if any.
pub fn read_key_file() -> Option<String> {
    let key = std::fs::read_to_string(key_path()?).ok()?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

/// Stores the key, or removes it when `None`.
pub fn write_key_file(key: Option<&str>) -> Result<()> {
    let path = key_path()
        .ok_or_else(|| Error::Unsupported("there is no config folder to keep the key in".into()))?;
    let Some(key) = key.map(str::trim).filter(|k| !k.is_empty()) else {
        return match std::fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.into()),
            _ => Ok(()),
        };
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&path)?;
    #[cfg(unix)]
    {
        // An older file keeps its mode on open.
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(key.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn settings_pick_defaults() {
        let off = ExplainSettings::default();
        assert_eq!(off.endpoint(), None);
        let anthropic = ExplainSettings {
            provider: Provider::Anthropic,
            ..Default::default()
        };
        assert_eq!(
            anthropic.endpoint().unwrap(),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(anthropic.model().unwrap(), DEFAULT_ANTHROPIC_MODEL);
        let ollama = ExplainSettings {
            provider: Provider::OpenAi,
            base_url: " http://localhost:11434/v1/ ".into(),
            model: "llama3.2".into(),
        }
        .normalized()
        .unwrap();
        assert_eq!(
            ollama.endpoint().unwrap(),
            "http://localhost:11434/v1/chat/completions"
        );
        let openai = ExplainSettings {
            provider: Provider::OpenAi,
            ..Default::default()
        };
        assert_eq!(openai.model(), None);
        let bad = ExplainSettings {
            base_url: "localhost:11434".into(),
            ..Default::default()
        };
        assert!(bad.normalized().is_err());
        assert_eq!(serde_json::to_value(Provider::OpenAi).unwrap(), "openai");
    }

    #[test]
    fn cuts_at_lines() {
        let text = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let head = cut(&text, 50, false);
        assert!(head.starts_with("line 0\n") && head.ends_with("(the rest is left out)"));
        let tail = cut(&text, 50, true);
        assert!(tail.starts_with("… (earlier lines left out)\n") && tail.ends_with("line 99"));
        assert_eq!(cut("short", 50, false), "short");
    }

    #[test]
    fn prompts_hide_secrets() {
        let text = compose(&[Section::new("Logs", "login ok, password=hunter2")], "Why?");
        assert_eq!(text, "## Logs\n\nlogin ok, password=<hidden>\n\nWhy?");
    }

    #[test]
    fn reads_answers() {
        let anthropic = json!({
            "model": "claude-opus-5-5",
            "stop_reason": "end_turn",
            "content": [{ "type": "thinking", "thinking": "…" }, { "type": "text", "text": "The image is missing." }]
        });
        let answer = anthropic_answer(&anthropic, "x").unwrap();
        assert_eq!(answer.text, "The image is missing.");
        assert!(!answer.cut_short);
        let refused = json!({ "stop_reason": "refusal", "content": [] });
        assert!(
            anthropic_answer(&refused, "x")
                .unwrap_err()
                .to_string()
                .contains("declined")
        );
        let openai =
            json!({ "choices": [{ "message": { "content": "Out of memory." }, "finish_reason": "length" }] });
        let answer = openai_answer(&openai, "llama3.2").unwrap();
        assert_eq!(
            (answer.text.as_str(), answer.model.as_str(), answer.cut_short),
            ("Out of memory.", "llama3.2", true)
        );
        assert_eq!(
            error_message(
                401,
                r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#
            ),
            "the provider refused the API key (401): invalid x-api-key"
        );
    }

    /// A one-shot HTTP server that answers `body` and hands back the request it got.
    async fn serve_once(body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        (url, task)
    }

    #[tokio::test]
    async fn asks_anthropic() {
        let (url, server) = serve_once(
            r#"{"model":"claude-opus-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"Fix the image tag."}]}"#,
        )
        .await;
        let settings = ExplainSettings {
            provider: Provider::Anthropic,
            base_url: url,
            model: String::new(),
        };
        let answer = ask(&settings, Some("sk-test"), "Why? token=abc").await.unwrap();
        assert_eq!(answer.text, "Fix the image tag.");
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /v1/messages "));
        assert!(request.contains("x-api-key: sk-test"));
        assert!(request.contains("anthropic-version: 2023-06-01"));
        let body: Value = serde_json::from_str(&request[request.find("\r\n\r\n").unwrap() + 4..]).unwrap();
        assert_eq!(body["model"], DEFAULT_ANTHROPIC_MODEL);
        assert_eq!(body["messages"][0]["content"], "Why? token=<hidden>");
        assert!(body.get("temperature").is_none());
    }

    #[tokio::test]
    async fn asks_openai_compatible_endpoints_without_a_key() {
        let (url, server) = serve_once(
            r#"{"model":"llama3.2","choices":[{"message":{"content":"OOMKilled."},"finish_reason":"stop"}]}"#,
        )
        .await;
        let settings = ExplainSettings {
            provider: Provider::OpenAi,
            base_url: format!("{url}/v1"),
            model: "llama3.2".into(),
        };
        let answer = ask(&settings, None, "Why?").await.unwrap();
        assert_eq!(answer.text, "OOMKilled.");
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /v1/chat/completions "));
        assert!(!request.to_ascii_lowercase().contains("authorization"));
    }

    #[tokio::test]
    async fn anthropic_needs_a_key() {
        let settings = ExplainSettings {
            provider: Provider::Anthropic,
            ..Default::default()
        };
        assert!(
            ask(&settings, None, "Why?")
                .await
                .unwrap_err()
                .to_string()
                .contains("API key")
        );
        assert!(
            ask(&ExplainSettings::default(), None, "Why?")
                .await
                .unwrap_err()
                .to_string()
                .contains("off")
        );
    }
}
