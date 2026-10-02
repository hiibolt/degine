use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::config::LeanConfig;

use super::bridge::{self, DepGraph, LeanOutcome};

pub use crate::model::JobRequest;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LeanEvent {
    CompileStarted {
        target_rule_id: String,
    },
    GraphUpdated {
        target_rule_id: String,
        graph: DepGraph,
    },
    CompileFailed {
        target_rule_id: String,
        diagnostics: String,
    },
}

struct Work {
    job: JobRequest,
    reply: oneshot::Sender<Result<LeanOutcome>>,
}

/// Serial Lean worker. Edits never overlap on the generated `.lean` file.
#[derive(Clone)]
pub struct LeanQueue {
    tx: mpsc::UnboundedSender<Work>,
    events: broadcast::Sender<LeanEvent>,
}

impl LeanQueue {
    pub fn start(config: LeanConfig) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Work>();
        let (events, _) = broadcast::channel(256);
        let events_worker = events.clone();
        let config = Arc::new(config);
        tokio::spawn(async move {
            while let Some(work) = rx.recv().await {
                let target = work.job.target_rule_id.clone();
                let ephemeral = work.job.ephemeral;
                if !ephemeral {
                    let _ = events_worker.send(LeanEvent::CompileStarted {
                        target_rule_id: target.clone(),
                    });
                }
                let result = bridge::compile_and_extract(&config, &work.job).await;
                if ephemeral {
                    let _ = work.reply.send(result);
                    continue;
                }
                match &result {
                    Ok(LeanOutcome::Proved { graph }) => {
                        let _ = events_worker.send(LeanEvent::GraphUpdated {
                            target_rule_id: target,
                            graph: graph.clone(),
                        });
                    }
                    Ok(LeanOutcome::Invalid { diagnostics }) => {
                        let _ = events_worker.send(LeanEvent::CompileFailed {
                            target_rule_id: target,
                            diagnostics: diagnostics.clone(),
                        });
                    }
                    Err(err) => {
                        let _ = events_worker.send(LeanEvent::CompileFailed {
                            target_rule_id: target,
                            diagnostics: format!("{err:#}"),
                        });
                    }
                }
                let _ = work.reply.send(result);
            }
        });
        Self { tx, events }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LeanEvent> {
        self.events.subscribe()
    }

    pub async fn submit(&self, job: JobRequest) -> Result<LeanOutcome> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Work {
                job,
                reply: reply_tx,
            })
            .context("lean worker is not running")?;
        reply_rx.await.context("lean worker dropped the job")?
    }
}
