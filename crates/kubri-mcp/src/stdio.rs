//! MCP over stdio: one JSON-RPC message per line on stdin and stdout. Logs go to stderr.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::{Mcp, parse_error};

/// Serves `mcp` until stdin closes. Messages are answered as they finish, so a ping is not
/// stuck behind a slow tool call.
pub async fn serve(mcp: Arc<Mcp>) -> std::io::Result<()> {
    let (answers, mut outbox) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = outbox.recv().await {
            stdout.write_all(line.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
        Ok::<_, std::io::Error>(())
    });
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut running = JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let mcp = mcp.clone();
        let answers = answers.clone();
        running.spawn(async move {
            let answer = match serde_json::from_str(&line) {
                Ok(message) => mcp.handle(message).await,
                Err(err) => Some(parse_error(&err.to_string())),
            };
            if let Some(answer) = answer {
                let _ = answers.send(answer.to_string());
            }
        });
        // Collect finished calls as we go.
        while running.try_join_next().is_some() {}
    }
    while running.join_next().await.is_some() {}
    drop(answers);
    writer.await.map_err(std::io::Error::other)?
}
