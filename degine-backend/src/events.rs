use serde::Serialize;

use crate::lean::{DepGraph, LeanEvent};
use crate::model::{Assert, Comment, Fact, Rule};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    FactChanged {
        fact: Fact,
    },
    FactDeleted {
        id: String,
    },
    RuleChanged {
        rule: Rule,
    },
    RuleDeleted {
        id: String,
    },
    AssertChanged {
        assert: Assert,
    },
    AssertDeleted {
        id: String,
    },
    CommentAdded {
        comment: Comment,
    },
    CommentChanged {
        comment: Comment,
    },
    CommentDeleted {
        id: i64,
        target_type: String,
        target_id: String,
    },
    AccessChanged,
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

impl From<LeanEvent> for ServerEvent {
    fn from(event: LeanEvent) -> Self {
        match event {
            LeanEvent::CompileStarted { target_rule_id } => Self::CompileStarted { target_rule_id },
            LeanEvent::GraphUpdated {
                target_rule_id,
                graph,
            } => Self::GraphUpdated {
                target_rule_id,
                graph,
            },
            LeanEvent::CompileFailed {
                target_rule_id,
                diagnostics,
            } => Self::CompileFailed {
                target_rule_id,
                diagnostics,
            },
        }
    }
}
