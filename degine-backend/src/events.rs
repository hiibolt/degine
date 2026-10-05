use serde::Serialize;

use crate::lean::{DepGraph, LeanEvent};
use crate::model::{Assert, Comment, Fact, Rule};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    FactChanged {
        workspace_id: String,
        fact: Fact,
    },
    FactDeleted {
        workspace_id: String,
        id: String,
    },
    RuleChanged {
        workspace_id: String,
        rule: Rule,
    },
    RuleDeleted {
        workspace_id: String,
        id: String,
    },
    AssertChanged {
        workspace_id: String,
        assert: Assert,
    },
    AssertDeleted {
        workspace_id: String,
        id: String,
    },
    CommentAdded {
        workspace_id: String,
        comment: Comment,
    },
    CommentChanged {
        workspace_id: String,
        comment: Comment,
    },
    CommentDeleted {
        workspace_id: String,
        id: i64,
        target_type: String,
        target_id: String,
    },
    AccessChanged {
        workspace_id: String,
    },
    CompileStarted {
        workspace_id: String,
        target_rule_id: String,
    },
    GraphUpdated {
        workspace_id: String,
        target_rule_id: String,
        graph: DepGraph,
    },
    CompileFailed {
        workspace_id: String,
        target_rule_id: String,
        diagnostics: String,
    },
}

impl ServerEvent {
    pub fn workspace_id(&self) -> &str {
        match self {
            Self::FactChanged { workspace_id, .. }
            | Self::FactDeleted { workspace_id, .. }
            | Self::RuleChanged { workspace_id, .. }
            | Self::RuleDeleted { workspace_id, .. }
            | Self::AssertChanged { workspace_id, .. }
            | Self::AssertDeleted { workspace_id, .. }
            | Self::CommentAdded { workspace_id, .. }
            | Self::CommentChanged { workspace_id, .. }
            | Self::CommentDeleted { workspace_id, .. }
            | Self::AccessChanged { workspace_id }
            | Self::CompileStarted { workspace_id, .. }
            | Self::GraphUpdated { workspace_id, .. }
            | Self::CompileFailed { workspace_id, .. } => workspace_id,
        }
    }
}

impl From<LeanEvent> for ServerEvent {
    fn from(event: LeanEvent) -> Self {
        match event {
            LeanEvent::CompileStarted {
                workspace_id,
                target_rule_id,
            } => Self::CompileStarted {
                workspace_id,
                target_rule_id,
            },
            LeanEvent::GraphUpdated {
                workspace_id,
                target_rule_id,
                graph,
            } => Self::GraphUpdated {
                workspace_id,
                target_rule_id,
                graph,
            },
            LeanEvent::CompileFailed {
                workspace_id,
                target_rule_id,
                diagnostics,
            } => Self::CompileFailed {
                workspace_id,
                target_rule_id,
                diagnostics,
            },
        }
    }
}
