//! The sanitizer's report as text and as SARIF.

use std::collections::BTreeMap;
use std::fmt::Write;

use kubri_core::{Finding, Issue, Level, SanitizerReport};
use serde_json::{Value, json};

fn level_word(level: Level) -> &'static str {
    match level {
        Level::Info => "info",
        Level::Warning => "warning",
        Level::Error => "error",
    }
}

fn object_name(finding: &Finding) -> String {
    match &finding.namespace {
        Some(namespace) => format!("{namespace}/{}", finding.name),
        None => finding.name.clone(),
    }
}

fn issue_text(issue: &Issue) -> String {
    match &issue.container {
        Some(container) => format!("container {container}: {}", issue.message),
        None => issue.message.clone(),
    }
}

/// The report for people: the score, then each kind with its problems, worst first. Infos are
/// left out; they do not lower the score.
pub fn text(report: &SanitizerReport, context: &str) -> String {
    let mut out = String::new();
    let scope = match &report.namespace {
        Some(namespace) => format!("namespace {namespace}"),
        None => "whole cluster".to_owned(),
    };
    let _ = writeln!(
        out,
        "{context}, {scope}: score {}, grade {}",
        report.score, report.grade
    );
    for section in &report.sections {
        if let Some(failed) = &section.failed {
            let _ = writeln!(out, "\n{}: not checked, {failed}", section.kind);
            continue;
        }
        if section.total == 0 {
            continue;
        }
        let mut counts = Vec::new();
        if section.error > 0 {
            counts.push(format!("{} with errors", section.error));
        }
        if section.warning > 0 {
            counts.push(format!("{} with warnings", section.warning));
        }
        let counts = if counts.is_empty() {
            "all fine".to_owned()
        } else {
            counts.join(", ")
        };
        let _ = writeln!(
            out,
            "\n{}: {} checked, {counts}, score {}",
            section.kind,
            section.total,
            section.score.unwrap_or_default()
        );
        for finding in &section.findings {
            for issue in finding.issues.iter().filter(|i| i.level > Level::Info) {
                let _ = writeln!(
                    out,
                    "  {:<7}  {}  {}  ({})",
                    level_word(issue.level),
                    object_name(finding),
                    issue_text(issue),
                    issue.code
                );
            }
        }
    }
    out
}

/// The report as SARIF 2.1.0, as GitHub code scanning reads it. Objects have no file, so each
/// result points at a made-up path, `<namespace or cluster>/<kind>/<name>`, which names the
/// object.
pub fn sarif(report: &SanitizerReport, context: &str) -> Value {
    let mut rules: BTreeMap<&str, Level> = BTreeMap::new();
    let mut results = Vec::new();
    for section in &report.sections {
        for finding in &section.findings {
            for issue in &finding.issues {
                let worst = rules.entry(issue.code).or_insert(issue.level);
                *worst = (*worst).max(issue.level);
                let place = finding.namespace.as_deref().unwrap_or("cluster");
                let uri = format!("{place}/{}/{}", section.kind, finding.name);
                results.push(json!({
                    "ruleId": issue.code,
                    "level": sarif_level(issue.level),
                    "message": {
                        "text": format!("{} {}: {}", section.kind, object_name(finding), issue_text(issue))
                    },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": uri, "uriBaseId": "CLUSTER" },
                            "region": { "startLine": 1 }
                        },
                        "logicalLocations": [{
                            "name": finding.name,
                            "fullyQualifiedName": uri,
                            "kind": "object"
                        }]
                    }],
                    "partialFingerprints": {
                        "kubriObject/v1": format!("{context}/{uri}/{}/{}", issue.code, issue.container.as_deref().unwrap_or(""))
                    }
                }));
            }
        }
    }
    let rules: Vec<Value> = rules
        .into_iter()
        .map(|(code, level)| {
            json!({
                "id": code,
                "name": code,
                "shortDescription": { "text": code.replace('-', " ") },
                "defaultConfiguration": { "level": sarif_level(level) }
            })
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "Kubri",
                    "informationUri": "https://github.com/snaketech-tu/kubri",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": rules
                }
            },
            "originalUriBaseIds": {
                "CLUSTER": { "description": { "text": format!("The objects of context {context}") } }
            },
            "properties": { "context": context, "score": report.score, "grade": report.grade.to_string() },
            "results": results
        }]
    })
}

fn sarif_level(level: Level) -> &'static str {
    match level {
        Level::Info => "note",
        Level::Warning => "warning",
        Level::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubri_core::{ResourceRef, SanitizerSection};

    fn report() -> SanitizerReport {
        let issue = |level, code, message: &str, container: Option<&str>| Issue {
            level,
            code,
            message: message.into(),
            container: container.map(Into::into),
        };
        SanitizerReport {
            score: 75,
            grade: 'C',
            namespace: Some("shop".into()),
            sections: vec![
                SanitizerSection {
                    kind: "Pod".into(),
                    resource: ResourceRef {
                        group: String::new(),
                        version: "v1".into(),
                        plural: "pods".into(),
                    },
                    total: 2,
                    ok: 1,
                    warning: 0,
                    error: 1,
                    score: Some(50),
                    findings: vec![Finding {
                        namespace: Some("shop".into()),
                        name: "api-1".into(),
                        level: Level::Error,
                        issues: vec![
                            issue(Level::Error, "crash-loop", "restarts all the time", Some("api")),
                            issue(Level::Info, "no-probe", "has no readiness probe", Some("api")),
                        ],
                    }],
                    failed: None,
                },
                SanitizerSection {
                    kind: "Secret".into(),
                    resource: ResourceRef {
                        group: String::new(),
                        version: "v1".into(),
                        plural: "secrets".into(),
                    },
                    total: 0,
                    ok: 0,
                    warning: 0,
                    error: 0,
                    score: None,
                    findings: vec![],
                    failed: Some("not allowed: needs list secrets".into()),
                },
            ],
        }
    }

    #[test]
    fn text_names_problems_but_not_infos() {
        let out = text(&report(), "prod");
        assert!(
            out.starts_with("prod, namespace shop: score 75, grade C\n"),
            "{out}"
        );
        assert!(out.contains("Pod: 2 checked, 1 with errors, score 50"), "{out}");
        assert!(
            out.contains("error    shop/api-1  container api: restarts all the time  (crash-loop)"),
            "{out}"
        );
        assert!(!out.contains("readiness"), "{out}");
        assert!(
            out.contains("Secret: not checked, not allowed: needs list secrets"),
            "{out}"
        );
    }

    #[test]
    fn sarif_has_a_rule_and_result_per_issue() {
        let doc = sarif(&report(), "prod");
        let run = &doc["runs"][0];
        assert_eq!(doc["version"], "2.1.0");
        let rules: Vec<&str> = run["tool"]["driver"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert_eq!(rules, ["crash-loop", "no-probe"]);
        let results = run["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["level"], "error");
        assert_eq!(results[1]["level"], "note");
        assert_eq!(
            results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "shop/Pod/api-1"
        );
        assert_eq!(
            results[0]["message"]["text"],
            "Pod shop/api-1: container api: restarts all the time"
        );
    }
}
