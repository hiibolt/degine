use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use postgres::{Client, Row, Transaction};
use rustls::ClientConfig;
use rustls::RootCertStore;
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::access::{Creator, Editor, Scope};
use crate::lean::DepGraph;
use crate::model::{Assert, Comment, Fact, Member, Person, PersonalFact, Rule, Workspace};
use crate::schema;

#[derive(Clone)]
pub struct Db {
    slots: Arc<Vec<Mutex<Client>>>,
    next: Arc<AtomicUsize>,
    emails: Arc<Mutex<HashMap<String, String>>>,
}

pub struct GraphRecord {
    pub status: String,
    pub graph: Option<DepGraph>,
    pub diagnostics: Option<String>,
}

impl Db {
    pub fn open(url: &str, ca_file: &str) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        if !ca_file.is_empty() {
            let mut reader = std::io::BufReader::new(
                std::fs::File::open(ca_file)
                    .with_context(|| format!("failed to open the database ca {ca_file}"))?,
            );
            for cert in rustls_pemfile::certs(&mut reader) {
                let cert = cert.context("the database ca was unreadable")?;
                roots
                    .add(cert)
                    .context("the database ca could not be trusted")?;
            }
        }
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = MakeRustlsConnect::new(config);
        let mut slots = Vec::new();
        for _ in 0..4 {
            slots.push(Mutex::new(
                Client::connect(url, connector.clone()).context("failed to open the database")?,
            ));
        }
        let db = Self {
            slots: Arc::new(slots),
            next: Arc::new(AtomicUsize::new(0)),
            emails: Arc::new(Mutex::new(HashMap::new())),
        };
        {
            let mut conn = db.lock()?;
            schema::migrate(&mut conn)?;
        }
        Ok(db)
    }

    fn hop<T: Send>(&self, f: impl FnOnce(&Db) -> Result<T> + Send) -> Result<T> {
        let db = self.clone();
        std::thread::scope(|scope| {
            scope
                .spawn(move || f(&db))
                .join()
                .map_err(|_| anyhow::anyhow!("database thread panicked"))?
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Client>> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
        self.slots[index]
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))
    }

    pub fn list_facts(&self, scope: &Scope) -> Result<Vec<Fact>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| db.list_facts_in(&ws).context("failed to list facts"))
    }

    pub(crate) fn list_facts_in(&self, ws: &str) -> Result<Vec<Fact>> {
        let mut conn = self.lock()?;
        let rows = conn.query(
            "SELECT id, claim, formula, role FROM facts WHERE workspace_id = $1 ORDER BY id",
            &[&ws],
        )?;
        let mut facts: Vec<Fact> = rows.iter().map(read_fact).collect::<Result<_, _>>()?;
        attach_citations(&mut conn, ws, &mut facts)?;
        Ok(facts)
    }

    pub fn fact(&self, scope: &Scope, id: &str) -> Result<Option<Fact>> {
        let ws = scope.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            db.fact_in(&ws, &id)
                .with_context(|| format!("failed to read fact `{id}`"))
        })
    }

    fn fact_in(&self, ws: &str, id: &str) -> Result<Option<Fact>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, claim, formula, role FROM facts WHERE workspace_id = $1 AND id = $2",
            &[&ws, &id],
        )?;
        let Some(row) = found else {
            return Ok(None);
        };
        let mut fact = read_fact(&row)?;
        fact.citations = citations_for(&mut conn, ws, id)?;
        Ok(Some(fact))
    }

    pub fn insert_fact(&self, editor: &Editor, fact: &Fact) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let fact = fact.clone();
        self.hop(move |db| {
            db.insert_fact_in(&ws, &user, &fact)
                .with_context(|| format!("failed to insert fact `{}`", fact.id))
        })
    }

    fn insert_fact_in(&self, ws: &str, user: &str, fact: &Fact) -> Result<()> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO facts (workspace_id, id, claim, formula, role, owner_id)
             VALUES ($1, $2, $3, $4, $5, $6::uuid)",
            &[&ws, &fact.id, &fact.claim, &fact.formula, &fact.role, &user],
        )?;
        write_citations(&mut tx, ws, &fact.id, &fact.citations)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_fact(&self, editor: &Editor, fact: &Fact) -> Result<bool> {
        let ws = editor.ws().to_string();
        let fact = fact.clone();
        self.hop(move |db| {
            db.update_fact_in(&ws, &fact)
                .with_context(|| format!("failed to update fact `{}`", fact.id))
        })
    }

    fn update_fact_in(&self, ws: &str, fact: &Fact) -> Result<bool> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE facts SET claim = $3, formula = $4, role = $5
             WHERE workspace_id = $1 AND id = $2",
            &[&ws, &fact.id, &fact.claim, &fact.formula, &fact.role],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        tx.execute(
            "DELETE FROM fact_citations WHERE workspace_id = $1 AND fact_id = $2",
            &[&ws, &fact.id],
        )?;
        write_citations(&mut tx, ws, &fact.id, &fact.citations)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn delete_fact(&self, editor: &Editor, id: &str) -> Result<bool> {
        let ws = editor.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            let changed = db
                .lock()?
                .execute(
                    "DELETE FROM facts WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &id],
                )
                .with_context(|| format!("failed to delete fact `{id}`"))?;
            Ok(changed > 0)
        })
    }

    pub fn list_rules(&self, scope: &Scope) -> Result<Vec<Rule>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| db.list_rules_in(&ws).context("failed to list rules"))
    }

    pub(crate) fn list_rules_in(&self, ws: &str) -> Result<Vec<Rule>> {
        let mut conn = self.lock()?;
        let rows = conn.query(
            "SELECT id, conclusion FROM rules WHERE workspace_id = $1 ORDER BY id",
            &[&ws],
        )?;
        let mut rules: Vec<Rule> = rows.iter().map(read_rule).collect::<Result<_, _>>()?;
        attach_premises(&mut conn, ws, &mut rules)?;
        Ok(rules)
    }

    pub fn rule(&self, scope: &Scope, id: &str) -> Result<Option<Rule>> {
        let ws = scope.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            db.rule_in(&ws, &id)
                .with_context(|| format!("failed to read rule `{id}`"))
        })
    }

    fn rule_in(&self, ws: &str, id: &str) -> Result<Option<Rule>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, conclusion FROM rules WHERE workspace_id = $1 AND id = $2",
            &[&ws, &id],
        )?;
        let Some(row) = found else {
            return Ok(None);
        };
        let mut rule = read_rule(&row)?;
        rule.premises = premises_for(&mut conn, ws, id)?;
        Ok(Some(rule))
    }

    pub fn insert_rule(&self, editor: &Editor, rule: &Rule) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let rule = rule.clone();
        self.hop(move |db| {
            db.insert_rule_in(&ws, &user, &rule)
                .with_context(|| format!("failed to insert rule `{}`", rule.id))
        })
    }

    fn insert_rule_in(&self, ws: &str, user: &str, rule: &Rule) -> Result<()> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO rules (workspace_id, id, conclusion, owner_id) VALUES ($1, $2, $3, $4::uuid)",
            &[&ws, &rule.id, &rule.conclusion, &user],
        )?;
        write_premises(&mut tx, ws, &rule.id, &rule.premises)?;
        tx.execute(
            "INSERT INTO graphs (workspace_id, rule_id, status) VALUES ($1, $2, 'pending')",
            &[&ws, &rule.id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_rule(&self, editor: &Editor, rule: &Rule) -> Result<bool> {
        let ws = editor.ws().to_string();
        let rule = rule.clone();
        self.hop(move |db| {
            db.update_rule_in(&ws, &rule)
                .with_context(|| format!("failed to update rule `{}`", rule.id))
        })
    }

    fn update_rule_in(&self, ws: &str, rule: &Rule) -> Result<bool> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE rules SET conclusion = $3 WHERE workspace_id = $1 AND id = $2",
            &[&ws, &rule.id, &rule.conclusion],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        tx.execute(
            "DELETE FROM rule_premises WHERE workspace_id = $1 AND rule_id = $2",
            &[&ws, &rule.id],
        )?;
        write_premises(&mut tx, ws, &rule.id, &rule.premises)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn list_labels(&self, scope: &Scope) -> Result<std::collections::BTreeMap<String, String>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, title FROM labels
                 WHERE workspace_id = $1 AND id NOT LIKE '~%' ORDER BY id",
                &[&ws],
            )?;
            let mut labels = std::collections::BTreeMap::new();
            for row in rows {
                labels.insert(row.get(0), row.get(1));
            }
            Ok(labels)
        })
    }

    pub fn delete_label(&self, editor: &Editor, id: &str) -> Result<Option<()>> {
        let ws = editor.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let found = conn
                .query_opt(
                    "SELECT id FROM labels WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &id],
                )
                .with_context(|| format!("failed to read label `{id}`"))?;
            if found.is_none() {
                return Ok(None);
            }
            if label_in_use(&mut conn, &ws, &id)? {
                anyhow::bail!("in use");
            }
            conn.execute(
                "DELETE FROM labels WHERE workspace_id = $1 AND id = $2",
                &[&ws, &id],
            )
            .with_context(|| format!("failed to delete label `{id}`"))?;
            Ok(Some(()))
        })
    }

    fn put_label(conn: &mut Client, ws: &str, user: &str, id: &str, title: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO labels (workspace_id, id, title, owner_id) VALUES ($1, $2, $3, $4::uuid)
             ON CONFLICT (workspace_id, id) DO UPDATE SET title = EXCLUDED.title",
            &[&ws, &id, &title, &user],
        )
        .with_context(|| format!("failed to write label `{id}`"))?;
        Ok(())
    }

    pub fn list_people(&self, scope: &Scope) -> Result<Vec<Person>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, title FROM labels
                 WHERE workspace_id = $1 AND id LIKE '~person:%' ORDER BY title",
                &[&ws],
            )?;
            let toggles = conn.query(
                "SELECT id FROM labels WHERE workspace_id = $1 AND id LIKE '~on:%'",
                &[&ws],
            )?;
            let mut on: HashMap<String, Vec<String>> = HashMap::new();
            for row in toggles {
                let id: String = row.get(0);
                let mut parts = id.splitn(3, ':');
                let _ = parts.next();
                let Some(person) = parts.next() else { continue };
                let Some(fact) = parts.next() else { continue };
                on.entry(person.to_string()).or_default().push(fact.to_string());
            }
            Ok(rows
                .into_iter()
                .map(|row| {
                    let raw: String = row.get(0);
                    let id = raw.trim_start_matches("~person:").to_string();
                    Person {
                        on: on.remove(&id).unwrap_or_default(),
                        id,
                        name: row.get(1),
                    }
                })
                .collect())
        })
    }

    pub fn insert_person(&self, editor: &Editor, id: &str, name: &str) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let id = id.to_string();
        let name = name.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            Self::put_label(&mut conn, &ws, &user, &format!("~person:{id}"), &name)
        })
    }

    pub fn delete_person(&self, editor: &Editor, id: &str) -> Result<bool> {
        let ws = editor.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let n = conn
                .execute(
                    "DELETE FROM labels WHERE workspace_id = $1 AND (id = $2 OR id LIKE $3)",
                    &[&ws, &format!("~person:{id}"), &format!("~on:{id}:%")],
                )
                .context("failed to delete a person")?;
            Ok(n > 0)
        })
    }

    pub fn list_personal_facts(&self, scope: &Scope) -> Result<Vec<PersonalFact>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, title FROM labels
                 WHERE workspace_id = $1 AND id LIKE '~pfact:%' ORDER BY title",
                &[&ws],
            )?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    let raw: String = row.get(0);
                    PersonalFact {
                        id: raw.trim_start_matches("~pfact:").to_string(),
                        claim: row.get(1),
                    }
                })
                .collect())
        })
    }

    pub fn insert_personal_fact(&self, editor: &Editor, id: &str, claim: &str) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let id = id.to_string();
        let claim = claim.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            Self::put_label(&mut conn, &ws, &user, &format!("~pfact:{id}"), &claim)
        })
    }

    pub fn delete_personal_fact(&self, editor: &Editor, id: &str) -> Result<bool> {
        let ws = editor.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let n = conn
                .execute(
                    "DELETE FROM labels WHERE workspace_id = $1 AND (id = $2 OR id LIKE $3)",
                    &[&ws, &format!("~pfact:{id}"), &format!("~on:%:{id}")],
                )
                .context("failed to delete a personal fact")?;
            Ok(n > 0)
        })
    }

    pub fn set_toggle(&self, editor: &Editor, person: &str, fact: &str, on: bool) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let person = person.to_string();
        let fact = fact.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let id = format!("~on:{person}:{fact}");
            if on {
                Self::put_label(&mut conn, &ws, &user, &id, "on")?;
            } else {
                conn.execute(
                    "DELETE FROM labels WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &id],
                )
                .context("failed to set a personal fact")?;
            }
            Ok(())
        })
    }

    pub fn upsert_labels(
        &self,
        editor: &Editor,
        labels: &std::collections::BTreeMap<String, String>,
    ) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let labels = labels.clone();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            for (id, title) in &labels {
                let title = title.trim();
                if id.is_empty() || title.is_empty() {
                    continue;
                }
                Self::put_label(&mut conn, &ws, &user, id, title)?;
            }
            Ok(())
        })
    }

    pub fn list_asserts(&self, scope: &Scope) -> Result<Vec<Assert>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| db.list_asserts_in(&ws).context("failed to list asserts"))
    }

    pub(crate) fn list_asserts_in(&self, ws: &str) -> Result<Vec<Assert>> {
        let mut conn = self.lock()?;
        let rows = conn.query(
            "SELECT id, title, description, formula FROM asserts WHERE workspace_id = $1 ORDER BY id",
            &[&ws],
        )?;
        let mut asserts = rows.iter().map(read_assert).collect::<Result<Vec<_>, _>>()?;
        attach_assumes(&mut conn, ws, &mut asserts)?;
        Ok(asserts)
    }

    pub fn get_assert(&self, scope: &Scope, id: &str) -> Result<Option<Assert>> {
        let ws = scope.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            db.get_assert_in(&ws, &id)
                .with_context(|| format!("failed to read assert `{id}`"))
        })
    }

    fn get_assert_in(&self, ws: &str, id: &str) -> Result<Option<Assert>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, title, description, formula FROM asserts WHERE workspace_id = $1 AND id = $2",
            &[&ws, &id],
        )?;
        let Some(row) = found else {
            return Ok(None);
        };
        let mut one = vec![read_assert(&row)?];
        attach_assumes(&mut conn, ws, &mut one)?;
        Ok(one.into_iter().next())
    }

    pub fn set_assert_assumes(&self, editor: &Editor, assert_id: &str, ids: &[String]) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let assert_id = assert_id.to_string();
        let ids = ids.to_vec();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let like = format!("~assume:{assert_id}:%");
            conn.execute(
                "DELETE FROM labels WHERE workspace_id = $1 AND id LIKE $2",
                &[&ws, &like],
            )?;
            for id in &ids {
                let label = format!("~assume:{assert_id}:{id}");
                Self::put_label(&mut conn, &ws, &user, &label, "on")?;
            }
            Ok(())
        })
    }

    pub fn insert_assert(&self, editor: &Editor, assert: &Assert) -> Result<()> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let assert = assert.clone();
        self.hop(move |db| {
            db.insert_assert_in(&ws, &user, &assert)
                .with_context(|| format!("failed to insert assert `{}`", assert.id))
        })
    }

    fn insert_assert_in(&self, ws: &str, user: &str, assert: &Assert) -> Result<()> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO asserts (workspace_id, id, title, description, formula, owner_id)
             VALUES ($1, $2, $3, $4, $5, $6::uuid)",
            &[
                &ws,
                &assert.id,
                &assert.title,
                &assert.description,
                &assert.formula,
                &user,
            ],
        )?;
        tx.execute(
            "INSERT INTO assert_graphs (workspace_id, assert_id, status) VALUES ($1, $2, 'pending')",
            &[&ws, &assert.id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_assert(&self, editor: &Editor, assert: &Assert) -> Result<bool> {
        let ws = editor.ws().to_string();
        let assert = assert.clone();
        self.hop(move |db| {
            db.update_assert_in(&ws, &assert)
                .with_context(|| format!("failed to update assert `{}`", assert.id))
        })
    }

    fn update_assert_in(&self, ws: &str, assert: &Assert) -> Result<bool> {
        let changed = self.lock()?.execute(
            "UPDATE asserts SET title = $3, description = $4, formula = $5
             WHERE workspace_id = $1 AND id = $2",
            &[
                &ws,
                &assert.id,
                &assert.title,
                &assert.description,
                &assert.formula,
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_assert(&self, editor: &Editor, id: &str) -> Result<bool> {
        let ws = editor.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            let changed = db
                .lock()?
                .execute(
                    "DELETE FROM asserts WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &id],
                )
                .with_context(|| format!("failed to delete assert `{id}`"))?;
            Ok(changed > 0)
        })
    }

    pub fn mark_assert_compiling(&self, ws: &str, id: &str) -> Result<()> {
        let ws = ws.to_string();
        let id = id.to_string();
        self.hop(move |db| {
            db.lock()?
                .execute(
                    "UPDATE assert_graphs SET status = 'pending' WHERE workspace_id = $1 AND assert_id = $2",
                    &[&ws, &id],
                )
                .with_context(|| format!("failed to mark assert `{id}` as compiling"))?;
            Ok(())
        })
    }

    pub fn record_assert_proved(&self, ws: &str, id: &str, graph: &DepGraph) -> Result<()> {
        let ws = ws.to_string();
        let id = id.to_string();
        let json = serde_json::to_string(graph).context("failed to encode the dependency graph")?;
        self.hop(move |db| {
            db.lock()?
                .execute(
                    "UPDATE assert_graphs
                     SET status = 'proved', graph_json = $3, diagnostics = NULL
                     WHERE workspace_id = $1 AND assert_id = $2",
                    &[&ws, &id, &json],
                )
                .with_context(|| format!("failed to store the graph for assert `{id}`"))?;
            Ok(())
        })
    }

    pub fn record_assert_invalid(&self, ws: &str, id: &str, diagnostics: &str) -> Result<()> {
        let ws = ws.to_string();
        let id = id.to_string();
        let diagnostics = diagnostics.to_string();
        self.hop(move |db| {
            db.lock()?
                .execute(
                    "UPDATE assert_graphs
                     SET status = 'invalid', graph_json = NULL, diagnostics = $3
                     WHERE workspace_id = $1 AND assert_id = $2",
                    &[&ws, &id, &diagnostics],
                )
                .with_context(|| format!("failed to store diagnostics for assert `{id}`"))?;
            Ok(())
        })
    }

    pub fn assert_graph(&self, scope: &Scope, id: &str) -> Result<Option<GraphRecord>> {
        let ws = scope.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            db.assert_graph_in(&ws, &id)
                .with_context(|| format!("failed to read the graph for assert `{id}`"))
        })
    }

    fn assert_graph_in(&self, ws: &str, id: &str) -> Result<Option<GraphRecord>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT status, graph_json, diagnostics FROM assert_graphs
             WHERE workspace_id = $1 AND assert_id = $2",
            &[&ws, &id],
        )?;
        graph_record(found)
    }

    pub fn delete_rule(&self, editor: &Editor, id: &str) -> Result<bool> {
        let ws = editor.ws().to_string();
        let id = id.to_string();
        self.hop(move |db| {
            let changed = db
                .lock()?
                .execute(
                    "DELETE FROM rules WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &id],
                )
                .with_context(|| format!("failed to delete rule `{id}`"))?;
            Ok(changed > 0)
        })
    }

    pub fn list_comments(&self, scope: &Scope, target_type: &str, target_id: &str) -> Result<Vec<Comment>> {
        let ws = scope.ws().to_string();
        let target_type = target_type.to_string();
        let target_id = target_id.to_string();
        self.hop(move |db| {
            db.list_comments_in(&ws, &target_type, &target_id)
                .context("failed to list comments")
        })
    }

    fn list_comments_in(&self, ws: &str, target_type: &str, target_id: &str) -> Result<Vec<Comment>> {
        let mut conn = self.lock()?;
        let rows = conn.query(
            "SELECT id, target_type, target_id, author, body, created_at, resolved
             FROM comments
             WHERE workspace_id = $1 AND target_type = $2 AND target_id = $3
             ORDER BY id",
            &[&ws, &target_type, &target_id],
        )?;
        rows.iter().map(read_comment).collect()
    }

    pub fn insert_comment(
        &self,
        scope: &Scope,
        target_type: &str,
        target_id: &str,
        author: &str,
        body: &str,
    ) -> Result<Comment> {
        let ws = scope.ws().to_string();
        let target_type = target_type.to_string();
        let target_id = target_id.to_string();
        let author = author.to_string();
        let body = body.to_string();
        self.hop(move |db| {
            db.insert_comment_in(&ws, &target_type, &target_id, &author, &body)
                .context("failed to insert comment")
        })
    }

    fn insert_comment_in(
        &self,
        ws: &str,
        target_type: &str,
        target_id: &str,
        author: &str,
        body: &str,
    ) -> Result<Comment> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let mut conn = self.lock()?;
        let id: i64 = conn
            .query_one(
                "INSERT INTO comments (workspace_id, target_type, target_id, author, body, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id",
                &[&ws, &target_type, &target_id, &author, &body, &created_at],
            )?
            .get(0);
        Ok(Comment {
            id,
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            author: author.to_string(),
            body: body.to_string(),
            created_at,
            resolved: false,
        })
    }

    pub fn comment(&self, scope: &Scope, id: i64) -> Result<Option<Comment>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            db.comment_in(&ws, id)
                .with_context(|| format!("failed to read comment {id}"))
        })
    }

    fn comment_in(&self, ws: &str, id: i64) -> Result<Option<Comment>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, target_type, target_id, author, body, created_at, resolved
             FROM comments WHERE workspace_id = $1 AND id = $2",
            &[&ws, &id],
        )?;
        found.map(|row| read_comment(&row)).transpose()
    }

    pub fn update_comment(&self, scope: &Scope, id: i64, body: &str) -> Result<Option<Comment>> {
        let ws = scope.ws().to_string();
        let body = body.to_string();
        self.hop(move |db| {
            db.update_comment_in(&ws, id, &body)
                .with_context(|| format!("failed to update comment {id}"))
        })
    }

    fn update_comment_in(&self, ws: &str, id: i64, body: &str) -> Result<Option<Comment>> {
        let changed = self.lock()?.execute(
            "UPDATE comments SET body = $3 WHERE workspace_id = $1 AND id = $2",
            &[&ws, &id, &body],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.comment_in(ws, id)
    }

    pub fn set_comment_resolved(&self, scope: &Scope, id: i64, resolved: bool) -> Result<Option<Comment>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            db.set_comment_resolved_in(&ws, id, resolved)
                .with_context(|| format!("failed to resolve comment {id}"))
        })
    }

    fn set_comment_resolved_in(&self, ws: &str, id: i64, resolved: bool) -> Result<Option<Comment>> {
        let changed = self.lock()?.execute(
            "UPDATE comments SET resolved = $3 WHERE workspace_id = $1 AND id = $2",
            &[&ws, &id, &resolved],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.comment_in(ws, id)
    }

    pub fn delete_comment(&self, scope: &Scope, id: i64) -> Result<bool> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            let changed = db
                .lock()?
                .execute(
                    "DELETE FROM comments WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &id],
                )
                .with_context(|| format!("failed to delete comment {id}"))?;
            Ok(changed > 0)
        })
    }

    pub fn list_open_comments(&self, scope: &Scope) -> Result<Vec<Comment>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, target_type, target_id, author, body, created_at, resolved
                 FROM comments WHERE workspace_id = $1 AND resolved = false ORDER BY id DESC",
                &[&ws],
            )?;
            rows.iter().map(read_comment).collect()
        })
    }

    pub fn mark_compiling(&self, ws: &str, rule_id: &str) -> Result<()> {
        let ws = ws.to_string();
        let rule_id = rule_id.to_string();
        self.hop(move |db| {
            db.lock()?
                .execute(
                    "UPDATE graphs SET status = 'pending' WHERE workspace_id = $1 AND rule_id = $2",
                    &[&ws, &rule_id],
                )
                .with_context(|| format!("failed to mark rule `{rule_id}` as compiling"))?;
            Ok(())
        })
    }

    pub fn record_proved(&self, ws: &str, rule_id: &str, graph: &DepGraph) -> Result<()> {
        let ws = ws.to_string();
        let rule_id = rule_id.to_string();
        let json = serde_json::to_string(graph).context("failed to encode the dependency graph")?;
        self.hop(move |db| {
            db.lock()?
                .execute(
                    "UPDATE graphs
                     SET status = 'proved', graph_json = $3, diagnostics = NULL
                     WHERE workspace_id = $1 AND rule_id = $2",
                    &[&ws, &rule_id, &json],
                )
                .with_context(|| format!("failed to store the graph for `{rule_id}`"))?;
            Ok(())
        })
    }

    pub fn record_invalid(&self, ws: &str, rule_id: &str, diagnostics: &str) -> Result<()> {
        let ws = ws.to_string();
        let rule_id = rule_id.to_string();
        let diagnostics = diagnostics.to_string();
        self.hop(move |db| {
            db.lock()?
                .execute(
                    "UPDATE graphs
                     SET status = 'invalid', graph_json = NULL, diagnostics = $3
                     WHERE workspace_id = $1 AND rule_id = $2",
                    &[&ws, &rule_id, &diagnostics],
                )
                .with_context(|| format!("failed to store diagnostics for `{rule_id}`"))?;
            Ok(())
        })
    }

    pub fn graph(&self, scope: &Scope, rule_id: &str) -> Result<Option<GraphRecord>> {
        let ws = scope.ws().to_string();
        let rule_id = rule_id.to_string();
        self.hop(move |db| {
            db.graph_in(&ws, &rule_id)
                .with_context(|| format!("failed to read the graph for `{rule_id}`"))
        })
    }

    fn graph_in(&self, ws: &str, rule_id: &str) -> Result<Option<GraphRecord>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT status, graph_json, diagnostics FROM graphs
             WHERE workspace_id = $1 AND rule_id = $2",
            &[&ws, &rule_id],
        )?;
        graph_record(found)
    }

    fn setting(&self, key: &str) -> Result<Option<String>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            Ok(conn
                .query_opt("SELECT value FROM settings WHERE key = $1", &[&key])?
                .map(|row| row.get(0)))
        })
    }

    fn put_setting(&self, key: &str, value: &str) -> Result<()> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                &[&key, &value],
            )?;
            Ok(())
        })
    }

    pub fn note_email(&self, user_id: &str, email: &str) -> Result<()> {
        if let Ok(guard) = self.emails.lock() {
            if guard.get(user_id).map(String::as_str) == Some(email) {
                return Ok(());
            }
        }
        self.put_setting(&format!("email:{user_id}"), email)?;
        if let Ok(mut guard) = self.emails.lock() {
            guard.insert(user_id.to_string(), email.to_string());
        }
        Ok(())
    }

    pub fn api_token(&self, user_id: &str) -> Result<String> {
        let key = format!("token:{user_id}");
        if let Some(token) = self.setting(&key)? {
            return Ok(token);
        }
        let token = mint_token(user_id);
        self.put_setting(&key, &token)?;
        Ok(token)
    }

    pub fn reset_api_token(&self, user_id: &str) -> Result<String> {
        let token = mint_token(user_id);
        self.put_setting(&format!("token:{user_id}"), &token)?;
        Ok(token)
    }

    pub fn person_for_token(&self, token: &str) -> Result<Option<(String, String)>> {
        let Some(rest) = token.strip_prefix("dg1.") else {
            return Ok(None);
        };
        let Some((user_id, _)) = rest.split_once('.') else {
            return Ok(None);
        };
        let Some(stored) = self.setting(&format!("token:{user_id}"))? else {
            return Ok(None);
        };
        if stored != token {
            return Ok(None);
        }
        let Some(email) = self.setting(&format!("email:{user_id}"))? else {
            return Ok(None);
        };
        Ok(Some((user_id.to_string(), email)))
    }

    /// The fact's id stays the conclusion atom. Its text and citations move onto a new theorem.
    pub fn derive_fact(&self, editor: &Editor, old_id: &str, new_id: &str, new_claim: &str) -> Result<Fact> {
        let ws = editor.ws().to_string();
        let user = editor.user_id().to_string();
        let old_id = old_id.to_string();
        let new_id = new_id.to_string();
        let new_claim = new_claim.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let mut tx = conn.transaction()?;
            let found = tx.query_opt(
                "SELECT claim, role FROM facts WHERE workspace_id = $1 AND id = $2",
                &[&ws, &old_id],
            )?;
            let Some(row) = found else {
                anyhow::bail!("missing");
            };
            let old_claim: String = row.get(0);
            let role: String = row.get(1);
            if role != "fact" {
                anyhow::bail!("not a fact");
            }
            if tx
                .query_opt(
                    "SELECT id FROM facts WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &new_id],
                )?
                .is_some()
                || tx
                    .query_opt(
                        "SELECT id FROM labels WHERE workspace_id = $1 AND id = $2",
                        &[&ws, &new_id],
                    )?
                    .is_some()
            {
                anyhow::bail!("taken");
            }
            let citations: Vec<String> = tx
                .query(
                    "SELECT reference FROM fact_citations
                     WHERE workspace_id = $1 AND fact_id = $2 ORDER BY position",
                    &[&ws, &old_id],
                )?
                .iter()
                .map(|row| row.get(0))
                .collect();
            tx.execute(
                "INSERT INTO facts (workspace_id, id, claim, formula, role, owner_id)
                 VALUES ($1, $2, $3, NULL, 'fact', $4::uuid)",
                &[&ws, &new_id, &new_claim, &user],
            )?;
            tx.execute(
                "DELETE FROM facts WHERE workspace_id = $1 AND id = $2",
                &[&ws, &old_id],
            )?;
            tx.execute(
                "INSERT INTO labels (workspace_id, id, title, owner_id) VALUES ($1, $2, $3, $4::uuid)
                 ON CONFLICT (workspace_id, id) DO UPDATE SET title = excluded.title",
                &[&ws, &old_id, &old_claim, &user],
            )?;
            let theorem_id = fresh_id(&mut tx, &ws, &format!("{old_id}_because"))?;
            let formula = format!("imp(fact:{new_id}, fact:{old_id})");
            tx.execute(
                "INSERT INTO facts (workspace_id, id, claim, formula, role, owner_id)
                 VALUES ($1, $2, $3, $4, 'theorem', $5::uuid)",
                &[&ws, &theorem_id, &old_claim, &formula, &user],
            )?;
            write_citations(&mut tx, &ws, &theorem_id, &citations)?;
            tx.commit()?;
            Ok(Fact {
                id: theorem_id,
                claim: old_claim,
                citations,
                formula: Some(formula),
                role: "theorem".into(),
            })
        })
    }

    pub fn upsert_profile(&self, user_id: &str, username: Option<&str>, email: &str) -> Result<()> {
        let user_id = user_id.to_string();
        let username = username.map(|name| name.to_string());
        let email = email.to_string();
        self.hop(move |db| {
            db.lock()?.execute(
                "INSERT INTO profiles (user_id, username, email) VALUES ($1, $2, $3)
                 ON CONFLICT (user_id) DO UPDATE SET
                   email = EXCLUDED.email,
                   username = CASE
                     WHEN EXCLUDED.username IS NULL THEN profiles.username
                     WHEN EXISTS (
                       SELECT 1 FROM profiles other
                       WHERE other.username = EXCLUDED.username AND other.user_id <> EXCLUDED.user_id
                     ) THEN profiles.username
                     ELSE EXCLUDED.username
                   END",
                &[&user_id, &username, &email],
            )?;
            Ok(())
        })
    }

    pub fn profile_username(&self, user_id: &str) -> Result<Option<String>> {
        let user_id = user_id.to_string();
        self.hop(move |db| {
            Ok(db
                .lock()?
                .query_opt("SELECT username FROM profiles WHERE user_id = $1", &[&user_id])?
                .and_then(|row| row.get(0)))
        })
    }

    pub fn user_by_username(&self, username: &str) -> Result<Option<String>> {
        let username = username.to_string();
        self.hop(move |db| {
            Ok(db
                .lock()?
                .query_opt("SELECT user_id FROM profiles WHERE username = $1", &[&username])?
                .map(|row| row.get(0)))
        })
    }

    pub fn workspace_ids(&self) -> Result<Vec<String>> {
        self.hop(|db| {
            let rows = db.lock()?.query("SELECT id FROM workspaces ORDER BY id", &[])?;
            Ok(rows.into_iter().map(|row| row.get(0)).collect())
        })
    }

    pub fn is_member(&self, user_id: &str, workspace_id: &str) -> Result<bool> {
        let user_id = user_id.to_string();
        let workspace_id = workspace_id.to_string();
        self.hop(move |db| {
            Ok(db
                .lock()?
                .query_opt(
                    "SELECT 1 FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
                    &[&workspace_id, &user_id],
                )?
                .is_some())
        })
    }

    /// `role` is `reader` or `editor`. The bool is whether this user created it.
    pub fn membership(&self, workspace_id: &str, user_id: &str) -> Result<Option<(String, bool)>> {
        let workspace_id = workspace_id.to_string();
        let user_id = user_id.to_string();
        self.hop(move |db| {
            let row = db.lock()?.query_opt(
                "SELECT m.role, w.created_by = m.user_id
                 FROM workspace_members m
                 JOIN workspaces w ON w.id = m.workspace_id
                 WHERE m.workspace_id = $1 AND m.user_id = $2",
                &[&workspace_id, &user_id],
            )?;
            Ok(row.map(|row| (row.get(0), row.get(1))))
        })
    }

    pub fn list_workspaces(&self, user_id: &str) -> Result<Vec<Workspace>> {
        let user_id = user_id.to_string();
        self.hop(move |db| {
            let rows = db.lock()?.query(
                "SELECT w.id, w.name, m.role, w.created_by = m.user_id
                 FROM workspaces w
                 JOIN workspace_members m ON m.workspace_id = w.id
                 WHERE m.user_id = $1
                 ORDER BY w.name, w.id",
                &[&user_id],
            )?;
            Ok(rows
                .into_iter()
                .map(|row| Workspace {
                    id: row.get(0),
                    name: row.get(1),
                    role: row.get(2),
                    creator: row.get(3),
                })
                .collect())
        })
    }

    pub fn create_workspace(&self, user_id: &str, name: &str) -> Result<Workspace> {
        let user_id = user_id.to_string();
        let name = name.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let id = format!("ws_{}", mint_id(6));
            let created_at = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO workspaces (id, name, created_by, created_at) VALUES ($1, $2, $3, $4)",
                &[&id, &name, &user_id, &created_at],
            )?;
            conn.execute(
                "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'editor')",
                &[&id, &user_id],
            )?;
            Ok(Workspace {
                id,
                name,
                role: "editor".into(),
                creator: true,
            })
        })
    }

    pub fn rename_workspace(&self, creator: &Creator, name: &str) -> Result<()> {
        let ws = creator.ws().to_string();
        let user = creator.user_id().to_string();
        let name = name.to_string();
        self.hop(move |db| {
            let n = db.lock()?.execute(
                "UPDATE workspaces SET name = $2 WHERE id = $1 AND created_by = $3",
                &[&ws, &name, &user],
            )?;
            if n == 0 {
                anyhow::bail!("missing");
            }
            Ok(())
        })
    }

    pub fn delete_workspace(&self, creator: &Creator) -> Result<()> {
        let ws = creator.ws().to_string();
        let user = creator.user_id().to_string();
        self.hop(move |db| {
            let n = db.lock()?.execute(
                "DELETE FROM workspaces WHERE id = $1 AND created_by = $2",
                &[&ws, &user],
            )?;
            if n == 0 {
                anyhow::bail!("missing");
            }
            Ok(())
        })
    }

    pub fn leave_workspace(&self, scope: &Scope) -> Result<()> {
        if scope.creator() {
            anyhow::bail!("creator");
        }
        let ws = scope.ws().to_string();
        let user = scope.user_id().to_string();
        self.hop(move |db| {
            db.lock()?.execute(
                "DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
                &[&ws, &user],
            )?;
            Ok(())
        })
    }

    pub fn list_members(&self, scope: &Scope) -> Result<Vec<Member>> {
        let ws = scope.ws().to_string();
        self.hop(move |db| {
            let rows = db.lock()?.query(
                "SELECT m.user_id, p.username, m.role, w.created_by = m.user_id
                 FROM workspace_members m
                 JOIN workspaces w ON w.id = m.workspace_id
                 LEFT JOIN profiles p ON p.user_id = m.user_id
                 WHERE m.workspace_id = $1
                 ORDER BY p.username NULLS LAST, m.user_id",
                &[&ws],
            )?;
            Ok(rows
                .into_iter()
                .map(|row| Member {
                    user_id: row.get(0),
                    username: row.get(1),
                    role: row.get(2),
                    creator: row.get(3),
                })
                .collect())
        })
    }

    pub fn add_member(&self, creator: &Creator, user_id: &str, role: &str) -> Result<()> {
        let ws = creator.ws().to_string();
        let user_id = user_id.to_string();
        let role = role.to_string();
        self.hop(move |db| {
            let n = db.lock()?.execute(
                "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3)
                 ON CONFLICT DO NOTHING",
                &[&ws, &user_id, &role],
            )?;
            if n == 0 {
                anyhow::bail!("already");
            }
            Ok(())
        })
    }

    pub fn set_member_role(&self, creator: &Creator, user_id: &str, role: &str) -> Result<()> {
        let ws = creator.ws().to_string();
        let actor = creator.user_id().to_string();
        let user_id = user_id.to_string();
        let role = role.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let created: Option<String> = conn
                .query_opt("SELECT created_by FROM workspaces WHERE id = $1", &[&ws])?
                .map(|row| row.get(0));
            if created.as_deref() != Some(actor.as_str()) {
                anyhow::bail!("missing");
            }
            if created.as_deref() == Some(user_id.as_str()) {
                anyhow::bail!("creator");
            }
            let n = conn.execute(
                "UPDATE workspace_members SET role = $3 WHERE workspace_id = $1 AND user_id = $2",
                &[&ws, &user_id, &role],
            )?;
            if n == 0 {
                anyhow::bail!("missing");
            }
            Ok(())
        })
    }

    pub fn remove_member(&self, creator: &Creator, user_id: &str) -> Result<()> {
        let ws = creator.ws().to_string();
        let user_id = user_id.to_string();
        self.hop(move |db| {
            let mut conn = db.lock()?;
            let created: Option<String> = conn
                .query_opt("SELECT created_by FROM workspaces WHERE id = $1", &[&ws])?
                .map(|row| row.get(0));
            if created.as_deref() == Some(user_id.as_str()) {
                anyhow::bail!("creator");
            }
            let n = conn.execute(
                "DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
                &[&ws, &user_id],
            )?;
            if n == 0 {
                anyhow::bail!("missing");
            }
            Ok(())
        })
    }
}

fn graph_record(found: Option<Row>) -> Result<Option<GraphRecord>> {
    let Some(row) = found else {
        return Ok(None);
    };
    let status: String = row.get(0);
    let graph_json: Option<String> = row.get(1);
    let diagnostics: Option<String> = row.get(2);
    let graph = match graph_json {
        Some(text) => Some(serde_json::from_str(&text).context("stored graph is invalid json")?),
        None => None,
    };
    Ok(Some(GraphRecord {
        status,
        graph,
        diagnostics,
    }))
}

fn read_assert(row: &Row) -> Result<Assert> {
    Ok(Assert {
        id: row.get(0),
        title: row.get(1),
        description: row.get(2),
        formula: row.get(3),
        assumes: Vec::new(),
    })
}

fn attach_assumes(conn: &mut Client, ws: &str, asserts: &mut [Assert]) -> Result<()> {
    let rows = conn.query(
        "SELECT id FROM labels WHERE workspace_id = $1 AND id LIKE '~assume:%'",
        &[&ws],
    )?;
    for row in rows {
        let id: String = row.get(0);
        let Some(rest) = id.strip_prefix("~assume:") else { continue };
        let Some((assert_id, fact_id)) = rest.split_once(':') else { continue };
        if let Some(assert) = asserts.iter_mut().find(|item| item.id == assert_id) {
            assert.assumes.push(fact_id.to_string());
        }
    }
    Ok(())
}

fn read_fact(row: &Row) -> Result<Fact> {
    Ok(Fact {
        id: row.get(0),
        claim: row.get(1),
        citations: Vec::new(),
        formula: row.get(2),
        role: row.get(3),
    })
}

fn read_rule(row: &Row) -> Result<Rule> {
    Ok(Rule {
        id: row.get(0),
        premises: Vec::new(),
        conclusion: row.get(1),
    })
}

fn read_comment(row: &Row) -> Result<Comment> {
    Ok(Comment {
        id: row.get(0),
        target_type: row.get(1),
        target_id: row.get(2),
        author: row.get(3),
        body: row.get(4),
        created_at: row.get(5),
        resolved: row.get(6),
    })
}

fn citations_for(conn: &mut Client, ws: &str, fact_id: &str) -> Result<Vec<String>> {
    let rows = conn.query(
        "SELECT reference FROM fact_citations
         WHERE workspace_id = $1 AND fact_id = $2 ORDER BY position",
        &[&ws, &fact_id],
    )?;
    Ok(rows.iter().map(|row| row.get(0)).collect())
}

fn attach_citations(conn: &mut Client, ws: &str, facts: &mut [Fact]) -> Result<()> {
    let rows = conn.query(
        "SELECT fact_id, reference FROM fact_citations
         WHERE workspace_id = $1 ORDER BY fact_id, position",
        &[&ws],
    )?;
    let mut by_id: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for row in rows {
        by_id.entry(row.get(0)).or_default().push(row.get(1));
    }
    for fact in facts.iter_mut() {
        if let Some(citations) = by_id.remove(&fact.id) {
            fact.citations = citations;
        }
    }
    Ok(())
}

pub(crate) fn mint_id(bytes: usize) -> String {
    use std::io::Read;
    let mut bytes = vec![0u8; bytes];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut file| file.read_exact(&mut bytes));
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mint_token(user_id: &str) -> String {
    format!("dg1.{user_id}.{}", mint_id(16))
}

fn label_in_use(conn: &mut Client, ws: &str, id: &str) -> Result<bool> {
    let facts = conn.query(
        "SELECT formula FROM facts WHERE workspace_id = $1 AND formula IS NOT NULL",
        &[&ws],
    )?;
    for row in facts {
        let formula: String = row.get(0);
        if mentions_atom(&formula, id) {
            return Ok(true);
        }
    }
    let asserts = conn.query(
        "SELECT formula FROM asserts WHERE workspace_id = $1",
        &[&ws],
    )?;
    for row in asserts {
        let formula: String = row.get(0);
        if mentions_atom(&formula, id) {
            return Ok(true);
        }
    }
    let rules = conn.query(
        "SELECT conclusion FROM rules WHERE workspace_id = $1",
        &[&ws],
    )?;
    for row in rules {
        let conclusion: String = row.get(0);
        if mentions_atom(&conclusion, id) {
            return Ok(true);
        }
    }
    let premises = conn.query(
        "SELECT premise_id FROM rule_premises WHERE workspace_id = $1 AND premise_id = $2",
        &[&ws, &id],
    )?;
    Ok(!premises.is_empty())
}

fn mentions_atom(formula: &str, id: &str) -> bool {
    let needle = format!("fact:{id}");
    let mut rest = formula;
    while let Some(at) = rest.find(&needle) {
        let after = at + needle.len();
        let boundary = rest[after..].chars().next();
        if boundary
            .map(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
            .unwrap_or(true)
        {
            return true;
        }
        rest = &rest[after..];
    }
    false
}

fn fresh_id(tx: &mut Transaction<'_>, ws: &str, base: &str) -> Result<String> {
    let mut id = base.to_string();
    let mut n = 2;
    loop {
        let taken = tx
            .query_opt(
                "SELECT id FROM facts WHERE workspace_id = $1 AND id = $2",
                &[&ws, &id],
            )?
            .is_some()
            || tx
                .query_opt(
                    "SELECT id FROM labels WHERE workspace_id = $1 AND id = $2",
                    &[&ws, &id],
                )?
                .is_some();
        if !taken {
            return Ok(id);
        }
        id = format!("{base}_{n}");
        n += 1;
    }
}

fn write_citations(tx: &mut Transaction<'_>, ws: &str, fact_id: &str, citations: &[String]) -> Result<()> {
    for (position, reference) in citations.iter().enumerate() {
        let position = position as i32;
        tx.execute(
            "INSERT INTO fact_citations (workspace_id, fact_id, position, reference)
             VALUES ($1, $2, $3, $4)",
            &[&ws, &fact_id, &position, &reference],
        )?;
    }
    Ok(())
}

fn premises_for(conn: &mut Client, ws: &str, rule_id: &str) -> Result<Vec<String>> {
    let rows = conn.query(
        "SELECT premise_id FROM rule_premises
         WHERE workspace_id = $1 AND rule_id = $2 ORDER BY position",
        &[&ws, &rule_id],
    )?;
    Ok(rows.iter().map(|row| row.get(0)).collect())
}

fn attach_premises(conn: &mut Client, ws: &str, rules: &mut [Rule]) -> Result<()> {
    for rule in rules {
        rule.premises = premises_for(conn, ws, &rule.id)?;
    }
    Ok(())
}

fn write_premises(tx: &mut Transaction<'_>, ws: &str, rule_id: &str, premises: &[String]) -> Result<()> {
    for (position, premise) in premises.iter().enumerate() {
        let position = position as i32;
        tx.execute(
            "INSERT INTO rule_premises (workspace_id, rule_id, position, premise_id)
             VALUES ($1, $2, $3, $4)",
            &[&ws, &rule_id, &position, &premise],
        )?;
    }
    Ok(())
}

