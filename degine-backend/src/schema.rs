use anyhow::{Context, Result};
use postgres::Client;

use crate::db::mint_id;

pub fn migrate(conn: &mut Client) -> Result<()> {
    let already: bool = conn
        .query_opt(
            "SELECT 1 FROM information_schema.columns
             WHERE table_schema = 'degine' AND table_name = 'facts' AND column_name = 'workspace_id'",
            &[],
        )?
        .is_some();
    if already {
        return Ok(());
    }
    let row = conn.query_one(
        "SELECT current_user,
                (SELECT tableowner FROM pg_tables WHERE schemaname = 'degine' AND tablename = 'facts')",
        &[],
    )?;
    let who: String = row.get(0);
    let owner: String = row.get(1);
    let mut tx = conn
        .transaction()
        .context("failed to start the workspace migration")?;
    tx.batch_execute(
        "
        CREATE TABLE workspaces (
            id text PRIMARY KEY,
            name text NOT NULL,
            created_by text NOT NULL,
            created_at text NOT NULL
        );
        CREATE TABLE workspace_members (
            workspace_id text NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
            user_id text NOT NULL,
            role text NOT NULL CHECK (role IN ('reader', 'editor')),
            PRIMARY KEY (workspace_id, user_id)
        );
        CREATE TABLE profiles (
            user_id text PRIMARY KEY,
            username text,
            email text NOT NULL
        );
        CREATE UNIQUE INDEX profiles_username_key ON profiles (username) WHERE username IS NOT NULL;
        ",
    )
    .context(format!(
        "failed to create workspace tables (connected as {who}, facts owned by {owner}). those tables have to be created as postgres"
    ))?;

    let owner: Option<String> = tx
        .query_opt(
            "SELECT value FROM settings WHERE key = 'owner_user_id'",
            &[],
        )?
        .map(|row| row.get(0));
    let owner = match owner {
        Some(id) => id,
        None => tx
            .query_opt("SELECT owner_id::text FROM facts LIMIT 1", &[])?
            .map(|row| row.get(0))
            .unwrap_or_else(|| "owner".into()),
    };
    let workspace_id = format!("ws_{}", mint_id(6));
    let created_at = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO workspaces (id, name, created_by, created_at) VALUES ($1, 'library', $2, $3)",
        &[&workspace_id, &owner, &created_at],
    )?;
    tx.execute(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'editor')",
        &[&workspace_id, &owner],
    )?;
    tx.execute(
        "INSERT INTO profiles (user_id, email)
         SELECT substring(key from 7), value FROM settings WHERE key LIKE 'email:%'
         ON CONFLICT (user_id) DO NOTHING",
        &[],
    )?;
    let shares_exist: bool = tx
        .query_opt(
            "SELECT 1 FROM information_schema.tables
             WHERE table_schema = 'degine' AND table_name = 'shares'",
            &[],
        )?
        .is_some();
    if shares_exist {
        tx.execute(
            "INSERT INTO workspace_members (workspace_id, user_id, role)
             SELECT $1, substring(s.key from 7), 'reader'
             FROM settings s
             JOIN shares sh ON lower(sh.email) = lower(s.value)
             WHERE s.key LIKE 'email:%' AND substring(s.key from 7) <> $2
             ON CONFLICT DO NOTHING",
            &[&workspace_id, &owner],
        )
        .context("failed to keep old assert shares as readers")?;
        tx.execute("DROP TABLE shares", &[])?;
    }

    for table in [
        "facts",
        "rules",
        "labels",
        "asserts",
        "comments",
        "fact_citations",
        "rule_premises",
        "graphs",
        "assert_graphs",
    ] {
        tx.execute(
            &format!("ALTER TABLE {table} ADD COLUMN workspace_id text"),
            &[],
        )
        .with_context(|| format!("failed to add workspace_id to {table}"))?;
        tx.execute(
            &format!("UPDATE {table} SET workspace_id = $1 WHERE workspace_id IS NULL"),
            &[&workspace_id],
        )?;
        tx.execute(
            &format!("ALTER TABLE {table} ALTER COLUMN workspace_id SET NOT NULL"),
            &[],
        )?;
        tx.execute(
            &format!(
                "ALTER TABLE {table} ADD CONSTRAINT {table}_workspace_fkey
                 FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE"
            ),
            &[],
        )?;
    }

    tx.batch_execute(
        "
        ALTER TABLE fact_citations DROP CONSTRAINT fact_citations_fact_id_fkey;
        ALTER TABLE graphs DROP CONSTRAINT graphs_rule_id_fkey;
        ALTER TABLE rule_premises DROP CONSTRAINT rule_premises_rule_id_fkey;
        ALTER TABLE assert_graphs DROP CONSTRAINT assert_graphs_assert_id_fkey;

        ALTER TABLE facts DROP CONSTRAINT facts_pkey;
        ALTER TABLE facts ADD PRIMARY KEY (workspace_id, id);
        ALTER TABLE rules DROP CONSTRAINT rules_pkey;
        ALTER TABLE rules ADD PRIMARY KEY (workspace_id, id);
        ALTER TABLE labels DROP CONSTRAINT labels_pkey;
        ALTER TABLE labels ADD PRIMARY KEY (workspace_id, id);
        ALTER TABLE asserts DROP CONSTRAINT asserts_pkey;
        ALTER TABLE asserts ADD PRIMARY KEY (workspace_id, id);

        ALTER TABLE fact_citations DROP CONSTRAINT fact_citations_pkey;
        ALTER TABLE fact_citations ADD PRIMARY KEY (workspace_id, fact_id, position);
        ALTER TABLE fact_citations ADD CONSTRAINT fact_citations_fact_fkey
            FOREIGN KEY (workspace_id, fact_id) REFERENCES facts (workspace_id, id) ON DELETE CASCADE;

        ALTER TABLE graphs DROP CONSTRAINT graphs_pkey;
        ALTER TABLE graphs ADD PRIMARY KEY (workspace_id, rule_id);
        ALTER TABLE graphs ADD CONSTRAINT graphs_rule_fkey
            FOREIGN KEY (workspace_id, rule_id) REFERENCES rules (workspace_id, id) ON DELETE CASCADE;

        ALTER TABLE rule_premises DROP CONSTRAINT rule_premises_pkey;
        ALTER TABLE rule_premises ADD PRIMARY KEY (workspace_id, rule_id, position);
        ALTER TABLE rule_premises ADD CONSTRAINT rule_premises_rule_fkey
            FOREIGN KEY (workspace_id, rule_id) REFERENCES rules (workspace_id, id) ON DELETE CASCADE;

        ALTER TABLE assert_graphs DROP CONSTRAINT assert_graphs_pkey;
        ALTER TABLE assert_graphs ADD PRIMARY KEY (workspace_id, assert_id);
        ALTER TABLE assert_graphs ADD CONSTRAINT assert_graphs_assert_fkey
            FOREIGN KEY (workspace_id, assert_id) REFERENCES asserts (workspace_id, id) ON DELETE CASCADE;

        ALTER TABLE facts ALTER COLUMN owner_id DROP DEFAULT;
        ALTER TABLE rules ALTER COLUMN owner_id DROP DEFAULT;
        ALTER TABLE labels ALTER COLUMN owner_id DROP DEFAULT;
        ALTER TABLE asserts ALTER COLUMN owner_id DROP DEFAULT;

        CREATE INDEX comments_by_target ON comments (workspace_id, target_type, target_id);
        ",
    )
    .context("failed to scope existing rows to the workspace")?;
    tx.commit()
        .context("failed to commit the workspace migration")?;
    Ok(())
}
