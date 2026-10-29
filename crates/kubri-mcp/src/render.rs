//! Results as text an agent reads well: tables like `kubectl get`, YAML, and a size limit.

use kubri_core::{Column, Row};
use serde_json::Value;

/// Results are cut beyond this many bytes, so one call cannot flood the agent's context.
const MAX_RESULT: usize = 60_000;

/// Cuts `text` to [`MAX_RESULT`] bytes at a line break, saying so.
pub(crate) fn limit(text: String) -> String {
    if text.len() <= MAX_RESULT {
        return text;
    }
    let mut end = MAX_RESULT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let cut = text[..end].rfind('\n').unwrap_or(end);
    format!(
        "{}\n… cut after {} of {} bytes; ask for less, for example with a label selector, a namespace or fewer log lines.",
        &text[..cut],
        cut,
        text.len()
    )
}

fn cell(value: &Value) -> String {
    match value {
        Value::Null => "<none>".into(),
        Value::String(s) if s.is_empty() => "<none>".into(),
        Value::String(s) => s.replace('\n', " "),
        other => other.to_string(),
    }
}

/// A table as `kubectl get` prints it, with the namespace first when there is one, and the
/// wide columns when `wide`.
pub(crate) fn table(columns: &[Column], rows: &[&Row], wide: bool) -> String {
    let shown: Vec<usize> = (0..columns.len())
        .filter(|&i| wide || columns[i].priority == 0)
        .collect();
    let namespaced = rows.iter().any(|r| r.namespace.is_some());
    let mut lines: Vec<Vec<String>> = Vec::with_capacity(rows.len() + 1);
    let mut head: Vec<String> = Vec::new();
    if namespaced {
        head.push("NAMESPACE".into());
    }
    head.extend(shown.iter().map(|&i| columns[i].name.to_uppercase()));
    lines.push(head);
    for row in rows {
        let mut line = Vec::new();
        if namespaced {
            line.push(row.namespace.clone().unwrap_or_default());
        }
        line.extend(
            shown
                .iter()
                .map(|&i| row.cells.get(i).map(cell).unwrap_or_default()),
        );
        lines.push(line);
    }
    let widths: Vec<usize> = (0..lines[0].len())
        .map(|i| lines.iter().map(|l| l[i].chars().count()).max().unwrap_or(0))
        .collect();
    lines
        .iter()
        .map(|line| {
            let padded: Vec<String> = line
                .iter()
                .zip(&widths)
                .map(|(text, width)| format!("{text:<width$}"))
                .collect();
            padded.join("   ").trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A value as YAML.
pub(crate) fn yaml(value: &impl serde::Serialize) -> String {
    serde_saphyr::to_string(value).unwrap_or_else(|e| format!("cannot write YAML: {e}"))
}

/// Unix seconds as an RFC 3339 time.
pub(crate) fn unix_time(seconds: f64) -> String {
    k8s_openapi::jiff::Timestamp::from_millisecond((seconds * 1000.0) as i64)
        .map(|t| t.to_string())
        .unwrap_or_else(|_| seconds.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn column(name: &str, priority: i32) -> Column {
        serde_json::from_value(json!({ "name": name, "type": "string", "priority": priority })).unwrap()
    }

    fn row(namespace: Option<&str>, cells: Vec<Value>) -> Row {
        Row {
            key: String::new(),
            name: cells[0].as_str().unwrap_or_default().to_owned(),
            namespace: namespace.map(Into::into),
            created: None,
            labels: Default::default(),
            owners: Vec::new(),
            cells,
        }
    }

    #[test]
    fn prints_like_kubectl() {
        let columns = [column("Name", 0), column("Ready", 0), column("IP", 1)];
        let rows = [
            row(
                Some("shop"),
                vec![json!("web-1"), json!("1/1"), json!("10.0.0.1")],
            ),
            row(Some("default"), vec![json!("a"), json!(""), json!(null)]),
        ];
        let rows: Vec<&Row> = rows.iter().collect();
        assert_eq!(
            table(&columns, &rows, false),
            "NAMESPACE   NAME    READY\nshop        web-1   1/1\ndefault     a       <none>"
        );
        assert!(table(&columns, &rows, true).starts_with("NAMESPACE   NAME    READY    IP"));
    }

    #[test]
    fn limits_long_results_at_a_line() {
        let text = "line\n".repeat(20_000);
        let cut = limit(text);
        assert!(cut.len() < MAX_RESULT + 200);
        assert!(cut.contains("… cut after"));
        assert!(cut.split('\n').next().unwrap() == "line");
    }
}
