mod asserts;
mod comments;
mod facts;
mod live;
mod people;
mod rules;
mod shares;

use anyhow::{Context, Result};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::routing::{get, post, put};
use axum::Router;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

use crate::access::grant_for;
use crate::auth::Auth;
use crate::config::Config;
use crate::db::{Db, GraphRecord};
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::lean::{DepGraph, JobRequest, LeanEvent, LeanQueue};
use crate::model::{Assert, Fact, Rule};

#[derive(Clone)]
struct AppState {
    db: Db,
    auth: Auth,
    queue: LeanQueue,
    events: broadcast::Sender<ServerEvent>,
}

struct Authed {
    id: String,
    email: String,
}

impl FromRequestParts<AppState> for Authed {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());
        let Some(token) = header.and_then(|value| value.strip_prefix("Bearer ")) else {
            return Err(AppError::unauthorized("unauthorized"));
        };
        if token.is_empty() {
            return Err(AppError::unauthorized("unauthorized"));
        }
        if let Some(person) = state.db.person_for_token(token)? {
            state.db.claim_owner(&person.0)?;
            return Ok(Authed {
                id: person.0,
                email: person.1,
            });
        }
        let person = state
            .auth
            .verify(token)
            .await
            .map_err(|_| AppError::unauthorized("unauthorized"))?;
        state.db.claim_owner(&person.id)?;
        state.db.note_email(&person.id, &person.email)?;
        Ok(Authed {
            id: person.id,
            email: person.email,
        })
    }
}

pub async fn serve(config: Config) -> Result<()> {
    let database_url = config.server.database_url.clone();
    let database_ca = config.server.database_ca.clone();
    let db = std::thread::spawn(move || Db::open(&database_url, &database_ca))
        .join()
        .map_err(|_| anyhow::anyhow!("database thread failed"))??;
    let queue = LeanQueue::start(config.lean);
    let (events, _) = broadcast::channel(256);
    let state = AppState {
        db,
        auth: Auth::new(&config.auth.supabase_url),
        queue,
        events,
    };
    forward_lean(&state);
    enqueue_debate(&state)?;
    let app = router(state);
    let listener = TcpListener::bind(&config.server.bind)
        .await
        .with_context(|| format!("failed to bind {}", config.server.bind))?;
    tracing::info!(bind = %config.server.bind, "listening");
    axum::serve(listener, app)
        .await
        .context("http server stopped")?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/me", get(live::me))
        .route("/account/token", get(live::show_token).post(live::reset_token))
        .route("/facts/{id}/derive", post(facts::derive_fact))
        .route("/ws", get(live::ws))
        .route("/facts", get(facts::list_facts).post(facts::create_fact))
        .route(
            "/facts/{id}",
            put(facts::update_fact).delete(facts::delete_fact),
        )
        .route("/rules", get(rules::list_rules).post(rules::create_rule))
        .route(
            "/rules/{id}",
            put(rules::update_rule).delete(rules::delete_rule),
        )
        .route("/rules/{id}/graph", get(rules::rule_graph))
        .route("/labels", get(facts::list_labels).put(facts::put_labels))
        .route("/labels/{id}", axum::routing::delete(facts::delete_label))
        .route(
            "/asserts",
            get(asserts::list_asserts).post(asserts::create_assert),
        )
        .route(
            "/asserts/{id}",
            put(asserts::update_assert).delete(asserts::delete_assert),
        )
        .route("/asserts/{id}/graph", get(asserts::assert_graph))
        .route("/asserts/{id}/for/{person}", get(people::check_person))
        .route("/people", get(people::list_people).post(people::create_person))
        .route("/people/{id}", axum::routing::delete(people::delete_person))
        .route(
            "/personal-facts",
            get(people::list_personal).post(people::create_personal),
        )
        .route(
            "/personal-facts/{id}",
            axum::routing::delete(people::delete_personal),
        )
        .route(
            "/people/{person}/toggles/{fact}",
            put(people::toggle),
        )
        .route("/asserts/{id}/shares", get(shares::list_shares))
        .route("/asserts/{id}/share", post(shares::grant_share))
        .route(
            "/asserts/{id}/share/{username}",
            axum::routing::delete(shares::revoke_share),
        )
        .route(
            "/comments",
            get(comments::list_comments).post(comments::create_comment),
        )
        .route(
            "/comments/{id}",
            put(comments::update_comment).delete(comments::delete_comment),
        )
        .route("/comments/{id}/resolved", post(comments::resolve_comment))
        .route("/inbox", get(comments::inbox))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

fn forward_lean(state: &AppState) {
    let mut rx = state.queue.subscribe();
    let db = state.db.clone();
    let events = state.events.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    record_lean(&db, &event);
                    let _ = events.send(ServerEvent::from(event));
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::error!("missed {skipped} lean events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn record_lean(db: &Db, event: &LeanEvent) {
    let result = match event {
        LeanEvent::GraphUpdated {
            target_rule_id,
            graph,
        } => {
            if db.get_assert(target_rule_id).ok().flatten().is_some() {
                db.record_assert_proved(target_rule_id, graph)
            } else {
                db.record_proved(target_rule_id, graph)
            }
        }
        LeanEvent::CompileFailed {
            target_rule_id,
            diagnostics,
        } => {
            if db.get_assert(target_rule_id).ok().flatten().is_some() {
                db.record_assert_invalid(target_rule_id, diagnostics)
            } else {
                db.record_invalid(target_rule_id, diagnostics)
            }
        }
        LeanEvent::CompileStarted { .. } => Ok(()),
    };
    if let Err(err) = result {
        tracing::error!("{err:#}");
    }
}

fn enqueue_debate(state: &AppState) -> Result<()> {
    let facts = state.db.list_facts()?;
    let rules = state.db.list_rules()?;
    for rule in &rules {
        state.db.mark_compiling(&rule.id)?;
        publish(
            state,
            ServerEvent::CompileStarted {
                target_rule_id: rule.id.clone(),
            },
        );
        let job = JobRequest {
            facts: facts.clone(),
            rules: rules.clone(),
            target_rule_id: rule.id.clone(),
            ephemeral: false,
        };
        let queue = state.queue.clone();
        tokio::spawn(async move {
            if let Err(err) = queue.submit(job).await {
                tracing::error!("{err:#}");
            }
        });
    }
    let asserts = state.db.list_asserts()?;
    for assert in &asserts {
        state.db.mark_assert_compiling(&assert.id)?;
        publish(
            state,
            ServerEvent::CompileStarted {
                target_rule_id: assert.id.clone(),
            },
        );
        let job = assert_job(assert, &facts, &rules);
        let queue = state.queue.clone();
        tokio::spawn(async move {
            if let Err(err) = queue.submit(job).await {
                tracing::error!("{err:#}");
            }
        });
    }
    Ok(())
}

fn assert_job(assert: &Assert, facts: &[Fact], rules: &[Rule]) -> JobRequest {
    let mut facts = facts.to_vec();
    for id in &assert.assumes {
        if let Some(fact) = facts.iter_mut().find(|fact| fact.id == *id) {
            fact.role = "fact".into();
        }
    }
    let mut premises: Vec<String> = facts
        .iter()
        .filter(|fact| fact.role != "criterion")
        .map(|fact| fact.id.clone())
        .collect();
    premises.extend(rules.iter().map(|rule| rule.id.clone()));
    let mut rules = rules.to_vec();
    rules.push(Rule {
        id: assert.id.clone(),
        premises,
        conclusion: assert.formula.clone(),
    });
    JobRequest {
        facts: facts.to_vec(),
        rules,
        target_rule_id: assert.id.clone(),
        ephemeral: false,
    }
}

fn publish(state: &AppState, event: ServerEvent) {
    let _ = state.events.send(event);
}

fn is_owner(state: &AppState, user_id: &str) -> Result<bool, AppError> {
    Ok(state.db.is_owner_id(user_id)?)
}

fn require_owner(state: &AppState, email: &str) -> Result<(), AppError> {
    if is_owner(state, email)? {
        Ok(())
    } else {
        Err(AppError::forbidden("only the library owner can change this"))
    }
}

fn can_see(
    state: &AppState,
    user_id: &str,
    username: &str,
    target_type: &str,
    target_id: &str,
) -> Result<bool, AppError> {
    if is_owner(state, user_id)? {
        return Ok(true);
    }
    let grant = grant_for(&state.db, username)?;
    Ok(grant.sees_target(target_type, target_id))
}

#[derive(Serialize)]
struct FactView {
    #[serde(flatten)]
    fact: Fact,
    shared: bool,
}

#[derive(Serialize)]
struct AssertView {
    #[serde(flatten)]
    assert: Assert,
    shared: bool,
}

#[derive(Serialize)]
struct GraphBody {
    status: String,
    graph: Option<DepGraph>,
    diagnostics: Option<String>,
}

fn graph_body(record: Option<GraphRecord>) -> GraphBody {
    match record {
        Some(record) => GraphBody {
            status: record.status,
            graph: record.graph,
            diagnostics: record.diagnostics,
        },
        None => GraphBody {
            status: "pending".into(),
            graph: None,
            diagnostics: None,
        },
    }
}

fn id_taken(state: &AppState, id: &str) -> Result<bool, AppError> {
    Ok(state.db.fact(id)?.is_some()
        || state.db.rule(id)?.is_some()
        || state.db.get_assert(id)?.is_some())
}

fn require_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(AppError::bad_request("id must be a non-empty token without spaces"));
    }
    Ok(())
}

fn clean_text(value: &str, label: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::bad_request(format!("{label} is required")));
    }
    Ok(value.to_string())
}

fn clean_citations(raw: Vec<String>) -> Vec<String> {
    raw.into_iter()
        .map(|citation| citation.trim().to_string())
        .filter(|citation| !citation.is_empty())
        .collect()
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let text = text.trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    })
}
