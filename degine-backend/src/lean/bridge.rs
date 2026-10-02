use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::timeout;

use crate::config::LeanConfig;

use crate::model::JobRequest;

use super::codegen::{self, Prepared};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepNode {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepGraph {
    pub nodes: Vec<DepNode>,
    pub edges: Vec<DepEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanOutcome {
    /// Lean accepted the chain. `graph` is the reachable fact/rule cone.
    Proved { graph: DepGraph },
    /// The chain is invalid. Diagnostics are for the client, not a server fault.
    Invalid { diagnostics: String },
}

pub(crate) async fn compile_and_extract(
    config: &LeanConfig,
    job: &JobRequest,
) -> Result<LeanOutcome> {
    let target = job.target_rule_id.clone();
    compile_and_extract_inner(config, job)
        .await
        .with_context(|| format!("failed to compile theorem {target}"))
}

async fn compile_and_extract_inner(config: &LeanConfig, job: &JobRequest) -> Result<LeanOutcome> {
    let prepared = codegen::prepare(job)?;
    let module = match prepared {
        Prepared::Rejected { diagnostics } => {
            return Ok(LeanOutcome::Invalid { diagnostics });
        }
        Prepared::Ready(module) => module,
    };

    let project_dir = absolute(&config.project_dir)?;
    let helper_path = absolute(&config.helper_path)?;
    let generated = project_dir.join("Debate").join("Generated.lean");
    write_atomic(&generated, &module.source)
        .with_context(|| format!("failed to write {}", generated.display()))?;

    tracing::info!(theorem = %module.target_decl, "building generated lean module");
    let build = run(config.timeout(), &project_dir, "lake", &["build", "Debate"]).await?;
    if !build.success {
        if build.combined.contains("Generated.lean") {
            return Ok(LeanOutcome::Invalid {
                diagnostics: build.combined,
            });
        }
        bail!("failed to build lean project: {}", build.combined);
    }

    let helper = run(
        config.timeout(),
        &project_dir,
        "lake",
        &["build", "extractor"],
    )
    .await?;
    if !helper.success {
        bail!("failed to build lean helper: {}", helper.combined);
    }

    tracing::info!(theorem = %module.target_decl, "extracting dependency graph");
    let extracted = run(
        config.timeout(),
        &project_dir,
        "lake",
        &["env", &helper_path.to_string_lossy(), &module.target_decl],
    )
    .await
    .context("failed to run lean helper")?;
    if !extracted.success {
        bail!("failed to run lean helper: {}", extracted.combined);
    }

    let mut graph: DepGraph = serde_json::from_str(extracted.stdout.trim()).with_context(|| {
        format!(
            "failed to parse extractor output: {}",
            extracted.stdout.trim()
        )
    })?;
    graph.nodes.sort_by(|a, b| a.name.cmp(&b.name));
    graph
        .edges
        .sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));
    graph.nodes.dedup();
    graph.edges.dedup();
    Ok(LeanOutcome::Proved { graph })
}

struct CmdOut {
    success: bool,
    combined: String,
    stdout: String,
}

async fn run(limit: Duration, cwd: &Path, program: &str, args: &[&str]) -> Result<CmdOut> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command
        .spawn()
        .with_context(|| format!("failed to spawn {program}"))?;
    let output = match timeout(limit, child.wait_with_output()).await {
        Ok(result) => result.with_context(|| format!("failed waiting for {program}"))?,
        Err(_) => bail!("timed out after {}s running {program}", limit.as_secs()),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
        if !stdout.ends_with('\n') {
            combined.push('\n');
        }
    }
    if !stderr.is_empty() {
        combined.push_str(&stderr);
    }
    Ok(CmdOut {
        success: output.status.success(),
        combined,
        stdout,
    })
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir().context("failed to read the working directory")?;
        Ok(cwd.join(path))
    }
}

fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path.parent().context("generated path has no parent")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    let tmp = parent.join(".Generated.lean.tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to move {} to {}", tmp.display(), path.display()))?;
    Ok(())
}
