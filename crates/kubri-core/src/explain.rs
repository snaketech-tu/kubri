//! Explain: sends what Kubri knows about a pod, an event, log lines, a finding or a failed action
//! to a model the user chose, with the user's own key or through Claude Code or Codex (see
//! [`crate::explain_cli`]), and returns the model's explanation.
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
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";
/// OpenRouter's router, which picks a model for each prompt, when the settings name none.
pub const DEFAULT_OPENROUTER_MODEL: &str = "openrouter/auto";
/// How OpenRouter names the app that asks, on its own pages.
const APP_URL: &str = "https://github.com/snaketech-tu/kubri";

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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename = "ExplainProvider")]
pub enum Provider {
    /// Explain is off: nothing is sent anywhere.
    #[default]
    None,
    /// The Anthropic Messages API.
    Anthropic,
    /// Any endpoint that speaks OpenAI's chat completions, such as Ollama or LM Studio.
    #[serde(rename = "openai")]
    OpenAi,
    /// OpenRouter, which reaches many providers' models with one key.
    #[serde(rename = "openrouter")]
    OpenRouter,
    /// Claude Code, run as `claude -p` with the account it is signed in to.
    Claude,
    /// Codex, run as `codex exec` with the account it is signed in to.
    Codex,
}

impl Provider {
    /// Whether it runs a command on this computer instead of calling an address.
    pub fn runs_command(self) -> bool {
        matches!(self, Provider::Claude | Provider::Codex)
    }

    /// The name of its key in the keychain and the config folder, for providers that take one.
    /// Each has its own, so a key is only ever sent to the provider it was saved for.
    pub fn key_name(self) -> Option<&'static str> {
        match self {
            Provider::Anthropic => Some("anthropic"),
            Provider::OpenAi => Some("openai"),
            Provider::OpenRouter => Some("openrouter"),
            Provider::None | Provider::Claude | Provider::Codex => None,
        }
    }
}

/// The model Explain asks and how to reach it. The key is kept apart, never in the settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
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
    /// Claude Code and Codex use their own default then.
    pub fn model(&self) -> Option<String> {
        match (self.provider, self.model.as_str()) {
            (_, model) if !model.is_empty() => Some(model.to_owned()),
            (Provider::Anthropic, _) => Some(DEFAULT_ANTHROPIC_MODEL.to_owned()),
            (Provider::OpenRouter, _) => Some(DEFAULT_OPENROUTER_MODEL.to_owned()),
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
            Provider::None | Provider::Claude | Provider::Codex => None,
            Provider::Anthropic => Some(format!("{}/v1/messages", base(ANTHROPIC_URL))),
            Provider::OpenAi => Some(format!("{}/chat/completions", base(OPENAI_URL))),
            Provider::OpenRouter => Some(format!("{}/chat/completions", base(OPENROUTER_URL))),
        }
    }
}

/// What to explain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExplainPrompt {
    /// A short name for what is explained, such as `Pod shop/web-1`.
    pub title: String,
    pub system: String,
    pub text: String,
    pub provider: Provider,
    /// `None` while Explain is off or no model is named.
    pub model: Option<String>,
    /// The address, or for Claude Code and Codex the program that runs.
    pub endpoint: Option<String>,
    /// Why it cannot be sent, when Kubri already knows: Claude Code or Codex cannot run here
    /// or is not installed.
    pub blocked: Option<String>,
    /// Whether a key is stored. Local models often need none.
    pub has_key: bool,
}

/// The model's explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
        let mut value = value.normalized()?;
        if value.provider.runs_command() {
            if let Some(reason) = self.explain_cli_refusal() {
                return Err(Error::Unsupported(reason.into()));
            }
            value.base_url.clear();
        }
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
        let (endpoint, blocked) = if settings.provider.runs_command() {
            match self.explain_cli(settings.provider).await {
                Ok(program) => (Some(program.display().to_string()), None),
                Err(err) => (None, Some(err.to_string())),
            }
        } else {
            (settings.endpoint(), None)
        };
        Ok(ExplainPrompt {
            title,
            system: SYSTEM_PROMPT.to_owned(),
            text: compose(&sections, question),
            provider: settings.provider,
            model: settings.model(),
            endpoint,
            blocked,
            has_key: false,
        })
    }

    /// Sends `text`, as the user saw it, to the model in the settings, with `key` if any, or
    /// runs Claude Code or Codex with it.
    pub async fn explain(&self, text: &str, key: Option<&str>) -> Result<ExplainAnswer> {
        let settings = self.settings().explain;
        if settings.provider.runs_command() {
            let program = self.explain_cli(settings.provider).await?;
            return crate::explain_cli::ask(
                settings.provider,
                &program,
                &settings.model,
                &redact::text(text),
            )
            .await;
        }
        ask(&settings, key, text).await
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
        402 => format!("no credit left ({status}): {message}"),
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
        Provider::OpenAi | Provider::OpenRouter => {
            let mut request = client.post(&endpoint).json(&json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": SYSTEM_PROMPT },
                    { "role": "user", "content": text },
                ],
            }));
            if settings.provider == Provider::OpenRouter {
                // OpenRouter takes no request without a key, and names the app with these.
                let key = key.ok_or_else(|| {
                    Error::Unsupported("Explain needs an OpenRouter API key in the settings".into())
                })?;
                request = request
                    .bearer_auth(key)
                    .header("HTTP-Referer", APP_URL)
                    .header("X-Title", "Kubri");
            } else if let Some(key) = key {
                request = request.bearer_auth(key);
            }
            request
        }
        Provider::None | Provider::Claude | Provider::Codex => {
            unreachable!("no endpoint while Explain is off or runs a command")
        }
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
    // OpenRouter can answer 200 with the error of the provider it asked, at the top or in the
    // choice.
    let choice = value.pointer("/choices/0");
    if let Some(error) = value
        .get("error")
        .or_else(|| choice.and_then(|c| c.get("error")))
        .filter(|e| !e.is_null())
    {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| error.to_string());
        return Err(model_error(format!(
            "the provider answered with an error: {message}"
        )));
    }
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

/// Where the API key of `provider` is kept when no keychain holds it: `explain-key-<provider>`
/// in Kubri's config folder, readable only by its owner.
pub fn key_path(provider: Provider) -> Option<PathBuf> {
    key_path_in(&crate::settings::dir()?, provider)
}

fn key_path_in(dir: &std::path::Path, provider: Provider) -> Option<PathBuf> {
    Some(dir.join(format!("explain-key-{}", provider.key_name()?)))
}

/// The stored key of `provider`, if any.
pub fn read_key_file(provider: Provider) -> Option<String> {
    read_key(&key_path(provider)?)
}

fn read_key(path: &std::path::Path) -> Option<String> {
    let key = std::fs::read_to_string(path).ok()?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

/// Stores the key of `provider`, or removes it when `None`.
pub fn write_key_file(provider: Provider, key: Option<&str>) -> Result<()> {
    let path = key_path(provider).ok_or_else(|| {
        Error::Unsupported("there is no config folder to keep the key in, or the provider takes none".into())
    })?;
    write_key(&path, key)
}

fn write_key(path: &std::path::Path, key: Option<&str>) -> Result<()> {
    let Some(key) = key.map(str::trim).filter(|k| !k.is_empty()) else {
        return match std::fs::remove_file(path) {
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
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        // An older file keeps its mode on open.
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(key.as_bytes())?;
    Ok(())
}

/// Gives the one key older versions kept for every provider (`explain-key`) to `owner`, the
/// provider in the settings, which is the one it was saved for. While the settings name no
/// provider that takes a key, it stays where it is, unused.
pub fn adopt_old_key_file(owner: Provider) {
    if let Some(dir) = crate::settings::dir() {
        adopt_old_key(&dir, owner);
    }
}

fn adopt_old_key(dir: &std::path::Path, owner: Provider) {
    let old = dir.join("explain-key");
    let Some(new) = key_path_in(dir, owner) else {
        return;
    };
    let Some(key) = read_key(&old) else {
        return;
    };
    if read_key(&new).is_none() && write_key(&new, Some(&key)).is_err() {
        return;
    }
    let _ = std::fs::remove_file(old);
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
        let openrouter = ExplainSettings {
            provider: Provider::OpenRouter,
            ..Default::default()
        };
        assert_eq!(
            openrouter.endpoint().unwrap(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(openrouter.model().unwrap(), DEFAULT_OPENROUTER_MODEL);
        assert_eq!(serde_json::to_value(Provider::OpenRouter).unwrap(), "openrouter");
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
    async fn asks_openrouter_with_its_key_and_kubris_name() {
        let (url, server) = serve_once(
            r#"{"model":"anthropic/claude-opus-5-5","choices":[{"message":{"content":"Raise the limit."},"finish_reason":"stop"}]}"#,
        )
        .await;
        let settings = ExplainSettings {
            provider: Provider::OpenRouter,
            base_url: format!("{url}/api/v1"),
            model: String::new(),
        };
        let answer = ask(&settings, Some("sk-or-v1-abc"), "Why?").await.unwrap();
        assert_eq!(answer.text, "Raise the limit.");
        assert_eq!(answer.model, "anthropic/claude-opus-5-5");
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /api/v1/chat/completions "), "{request}");
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer sk-or-v1-abc"), "{request}");
        assert!(lower.contains("x-title: kubri"), "{request}");
        assert!(request.contains(r#""model":"openrouter/auto""#), "{request}");
        let err = ask(&settings, None, "Why?").await.unwrap_err();
        assert!(err.to_string().contains("OpenRouter API key"), "{err}");
    }

    #[test]
    fn reads_errors_in_answers() {
        let err = openai_answer(
            &json!({"error": {"code": 502, "message": "Provider returned error"}}),
            "x",
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Explain: the provider answered with an error: Provider returned error"
        );
        let err = openai_answer(
            &json!({"choices": [{"error": {"message": "overloaded"}, "finish_reason": "error"}]}),
            "x",
        )
        .unwrap_err();
        assert!(err.to_string().ends_with("overloaded"));
        assert!(
            error_message(402, r#"{"error":{"message":"Insufficient credits"}}"#).starts_with("no credit")
        );
    }

    #[test]
    fn keeps_a_key_for_each_provider() {
        let dir = std::env::temp_dir().join(format!("kubri-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = |p| key_path_in(&dir, p).unwrap();
        write_key(&path(Provider::OpenRouter), Some(" sk-or \n")).unwrap();
        assert_eq!(read_key(&path(Provider::OpenRouter)).unwrap(), "sk-or");
        assert_eq!(read_key(&path(Provider::Anthropic)), None);
        assert_eq!(key_path_in(&dir, Provider::Claude), None);

        // The one key of older versions goes to the provider in the settings.
        std::fs::write(dir.join("explain-key"), "sk-ant").unwrap();
        adopt_old_key(&dir, Provider::None);
        assert!(
            dir.join("explain-key").exists(),
            "kept while no provider takes it"
        );
        adopt_old_key(&dir, Provider::Anthropic);
        assert_eq!(read_key(&path(Provider::Anthropic)).unwrap(), "sk-ant");
        assert!(!dir.join("explain-key").exists());
        assert_eq!(read_key(&path(Provider::OpenAi)), None);

        write_key(&path(Provider::OpenRouter), None).unwrap();
        assert_eq!(read_key(&path(Provider::OpenRouter)), None);
        std::fs::remove_dir_all(&dir).unwrap();
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
