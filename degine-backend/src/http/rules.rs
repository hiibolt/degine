use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::access::{Editor, Scope};
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Rule;

use super::{
    clean_text, enqueue_debate, graph_body, publish, require_id, AppState, GraphBody, IdPath,
};

pub(super) async fn list_rules(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<Rule>>, AppError> {
    Ok(Json(state.db.list_rules(&scope)?))
}

#[derive(Deserialize)]
pub(super) struct RuleWrite {
    premises: Vec<String>,
    conclusion: String,
}

#[derive(Deserialize)]
pub(super) struct NewRule {
    id: String,
    #[serde(flatten)]
    write: RuleWrite,
}

pub(super) async fn create_rule(
    State(state): State<AppState>,
    editor: Editor,
    Json(body): Json<NewRule>,
) -> Result<impl IntoResponse, AppError> {
    let rule = rule_from(body.id, body.write)?;
    if state.db.fact(&editor, &rule.id)?.is_some() || state.db.rule(&editor, &rule.id)?.is_some() {
        return Err(AppError::conflict(format!("id `{}` is already used", rule.id)));
    }
    state.db.insert_rule(&editor, &rule)?;
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::RuleChanged {
            workspace_id: ws.clone(),
            rule: rule.clone(),
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok((StatusCode::CREATED, Json(rule)))
}

pub(super) async fn update_rule(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
    Json(body): Json<RuleWrite>,
) -> Result<Json<Rule>, AppError> {
    let rule = rule_from(id, body)?;
    if state.db.fact(&editor, &rule.id)?.is_some() {
        return Err(AppError::conflict(format!(
            "id `{}` is already used by a fact",
            rule.id
        )));
    }
    if !state.db.update_rule(&editor, &rule)? {
        return Err(AppError::not_found());
    }
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::RuleChanged {
            workspace_id: ws.clone(),
            rule: rule.clone(),
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok(Json(rule))
}

pub(super) async fn delete_rule(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    if !state.db.delete_rule(&editor, &id)? {
        return Err(AppError::not_found());
    }
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::RuleDeleted {
            workspace_id: ws.clone(),
            id,
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn rule_graph(
    State(state): State<AppState>,
    scope: Scope,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<Json<GraphBody>, AppError> {
    if state.db.rule(&scope, &id)?.is_none() {
        return Err(AppError::not_found());
    }
    Ok(Json(graph_body(state.db.graph(&scope, &id)?)))
}

fn rule_from(id: String, body: RuleWrite) -> Result<Rule, AppError> {
    require_id(&id)?;
    let premises = body
        .premises
        .into_iter()
        .map(|premise| {
            let premise = premise.trim().to_string();
            if premise.is_empty() {
                Err(AppError::bad_request("a premise id is empty"))
            } else {
                Ok(premise)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Rule {
        id,
        premises,
        conclusion: clean_text(&body.conclusion, "conclusion")?,
    })
}
