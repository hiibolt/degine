-- Run this once in the Supabase SQL editor. It runs as postgres.
-- degine_app cannot create policies, publications, or exposed schemas.
-- After it succeeds, turn off "Allow public access" under Realtime settings
-- so only private channels can be joined.
-- If the authenticator step prints a notice, add degine under
-- Project Settings → Data API → Exposed schemas as well.

grant usage, create on schema degine to degine_app;

-- Workspace migration. Skipped once facts.workspace_id exists.
-- Tables stay owned by postgres through this block. Supabase will not let
-- postgres reassign them until it can set role degine_app, and the migration
-- itself has to run as the current owner.
do $$
declare
  library_owner text;
  ws text;
  shares_exist boolean;
begin
  if exists (
    select 1 from information_schema.columns
    where table_schema = 'degine' and table_name = 'facts' and column_name = 'workspace_id'
  ) then
    return;
  end if;

  create table degine.workspaces (
    id text primary key,
    name text not null,
    created_by text not null,
    created_at text not null
  );
  create table degine.workspace_members (
    workspace_id text not null references degine.workspaces(id) on delete cascade,
    user_id text not null,
    role text not null check (role in ('reader', 'editor')),
    primary key (workspace_id, user_id)
  );
  create table degine.profiles (
    user_id text primary key,
    username text,
    email text not null
  );
  create unique index profiles_username_key on degine.profiles (username) where username is not null;

  select coalesce(
    (select value from degine.settings where key = 'owner_user_id'),
    (select facts.owner_id::text from degine.facts limit 1)
  ) into library_owner;
  if library_owner is null then
    raise exception 'no owner to attach the library to';
  end if;

  ws := 'ws_' || substr(replace(gen_random_uuid()::text, '-', ''), 1, 12);
  insert into degine.workspaces (id, name, created_by, created_at)
  values (ws, 'library', library_owner, to_char(now() at time zone 'utc', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'));
  insert into degine.workspace_members (workspace_id, user_id, role)
  values (ws, library_owner, 'editor');

  insert into degine.profiles (user_id, email)
  select substring(key from 7), value from degine.settings where key like 'email:%'
  on conflict (user_id) do nothing;

  shares_exist := exists (
    select 1 from information_schema.tables
    where table_schema = 'degine' and table_name = 'shares'
  );
  if shares_exist then
    insert into degine.workspace_members (workspace_id, user_id, role)
    select ws, substring(s.key from 7), 'reader'
    from degine.settings s
    join degine.shares sh on lower(sh.email) = lower(s.value)
    where s.key like 'email:%' and substring(s.key from 7) <> library_owner
    on conflict do nothing;
    drop table degine.shares;
  end if;

  alter table degine.facts add column workspace_id text;
  alter table degine.rules add column workspace_id text;
  alter table degine.labels add column workspace_id text;
  alter table degine.asserts add column workspace_id text;
  alter table degine.comments add column workspace_id text;
  alter table degine.fact_citations add column workspace_id text;
  alter table degine.rule_premises add column workspace_id text;
  alter table degine.graphs add column workspace_id text;
  alter table degine.assert_graphs add column workspace_id text;

  update degine.facts set workspace_id = ws where workspace_id is null;
  update degine.rules set workspace_id = ws where workspace_id is null;
  update degine.labels set workspace_id = ws where workspace_id is null;
  update degine.asserts set workspace_id = ws where workspace_id is null;
  update degine.comments set workspace_id = ws where workspace_id is null;
  update degine.fact_citations set workspace_id = ws where workspace_id is null;
  update degine.rule_premises set workspace_id = ws where workspace_id is null;
  update degine.graphs set workspace_id = ws where workspace_id is null;
  update degine.assert_graphs set workspace_id = ws where workspace_id is null;

  alter table degine.facts alter column workspace_id set not null;
  alter table degine.rules alter column workspace_id set not null;
  alter table degine.labels alter column workspace_id set not null;
  alter table degine.asserts alter column workspace_id set not null;
  alter table degine.comments alter column workspace_id set not null;
  alter table degine.fact_citations alter column workspace_id set not null;
  alter table degine.rule_premises alter column workspace_id set not null;
  alter table degine.graphs alter column workspace_id set not null;
  alter table degine.assert_graphs alter column workspace_id set not null;

  alter table degine.facts add constraint facts_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.rules add constraint rules_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.labels add constraint labels_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.asserts add constraint asserts_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.comments add constraint comments_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.fact_citations add constraint fact_citations_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.rule_premises add constraint rule_premises_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.graphs add constraint graphs_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;
  alter table degine.assert_graphs add constraint assert_graphs_workspace_fkey
    foreign key (workspace_id) references degine.workspaces(id) on delete cascade;

  alter table degine.fact_citations drop constraint fact_citations_fact_id_fkey;
  alter table degine.graphs drop constraint graphs_rule_id_fkey;
  alter table degine.rule_premises drop constraint rule_premises_rule_id_fkey;
  alter table degine.assert_graphs drop constraint assert_graphs_assert_id_fkey;

  alter table degine.facts drop constraint facts_pkey;
  alter table degine.facts add primary key (workspace_id, id);
  alter table degine.rules drop constraint rules_pkey;
  alter table degine.rules add primary key (workspace_id, id);
  alter table degine.labels drop constraint labels_pkey;
  alter table degine.labels add primary key (workspace_id, id);
  alter table degine.asserts drop constraint asserts_pkey;
  alter table degine.asserts add primary key (workspace_id, id);

  alter table degine.fact_citations drop constraint fact_citations_pkey;
  alter table degine.fact_citations add primary key (workspace_id, fact_id, position);
  alter table degine.fact_citations add constraint fact_citations_fact_fkey
    foreign key (workspace_id, fact_id) references degine.facts (workspace_id, id) on delete cascade;

  alter table degine.graphs drop constraint graphs_pkey;
  alter table degine.graphs add primary key (workspace_id, rule_id);
  alter table degine.graphs add constraint graphs_rule_fkey
    foreign key (workspace_id, rule_id) references degine.rules (workspace_id, id) on delete cascade;

  alter table degine.rule_premises drop constraint rule_premises_pkey;
  alter table degine.rule_premises add primary key (workspace_id, rule_id, position);
  alter table degine.rule_premises add constraint rule_premises_rule_fkey
    foreign key (workspace_id, rule_id) references degine.rules (workspace_id, id) on delete cascade;

  alter table degine.assert_graphs drop constraint assert_graphs_pkey;
  alter table degine.assert_graphs add primary key (workspace_id, assert_id);
  alter table degine.assert_graphs add constraint assert_graphs_assert_fkey
    foreign key (workspace_id, assert_id) references degine.asserts (workspace_id, id) on delete cascade;

  alter table degine.facts alter column owner_id drop default;
  alter table degine.rules alter column owner_id drop default;
  alter table degine.labels alter column owner_id drop default;
  alter table degine.asserts alter column owner_id drop default;

  create index comments_by_target on degine.comments (workspace_id, target_type, target_id);
end $$;

-- Tables stay owned by postgres. The backend role bypasses row level security
-- on its own, and this role has to keep ownership so it can enable that
-- security and publish the tables.

create schema if not exists degine_private;
revoke all on schema degine_private from public, anon, authenticated;

-- Membership checks run as the function owner so policies do not recurse
-- through row level security on workspace_members. They still require the
-- caller to be the jwt user. degine_private is not an exposed schema.
create or replace function degine_private.is_member(ws text)
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select exists (
    select 1
    from degine.workspace_members
    where workspace_id = ws
      and user_id = (select auth.uid())::text
  );
$$;

create or replace function degine_private.shares_workspace(other_user text)
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select other_user = (select auth.uid())::text
    or exists (
      select 1
      from degine.workspace_members mine
      join degine.workspace_members theirs
        on theirs.workspace_id = mine.workspace_id
      where mine.user_id = (select auth.uid())::text
        and theirs.user_id = other_user
    );
$$;

revoke all on function degine_private.is_member(text) from public, anon;
revoke all on function degine_private.shares_workspace(text) from public, anon;
grant execute on function degine_private.is_member(text) to authenticated;
grant execute on function degine_private.shares_workspace(text) to authenticated;

revoke all on schema degine from public, anon, authenticated;
grant usage on schema degine to authenticated;
revoke create on schema degine from public, anon, authenticated;

revoke all on all tables in schema degine from public, anon, authenticated;

-- The revoke above removes grants that came through public. The backend role
-- is not the table owner, so it needs its own grants. It bypasses row level
-- security, and the column grants below are only for signed-in readers.
grant all on all tables in schema degine to degine_app;
grant all on all sequences in schema degine to degine_app;

grant select (workspace_id, id, claim, formula, role) on degine.facts to authenticated;
grant select (workspace_id, fact_id, position, reference) on degine.fact_citations to authenticated;
grant select (workspace_id, id, conclusion) on degine.rules to authenticated;
grant select (workspace_id, rule_id, position, premise_id) on degine.rule_premises to authenticated;
grant select (workspace_id, id, title) on degine.labels to authenticated;
grant select (workspace_id, id, title, description, formula) on degine.asserts to authenticated;
grant select (workspace_id, id, target_type, target_id, author, body, created_at, resolved)
  on degine.comments to authenticated;
grant select (workspace_id, rule_id, status, graph_json, diagnostics) on degine.graphs to authenticated;
grant select (workspace_id, assert_id, status, graph_json, diagnostics) on degine.assert_graphs to authenticated;
grant select (id, name, created_by, created_at) on degine.workspaces to authenticated;
grant select (workspace_id, user_id, role) on degine.workspace_members to authenticated;
grant select (user_id, username) on degine.profiles to authenticated;

-- settings holds emails and api tokens. No grants, and no policies, so the
-- Data API returns nothing. The backend role bypasses row level security.
alter table degine.facts enable row level security;
alter table degine.fact_citations enable row level security;
alter table degine.rules enable row level security;
alter table degine.rule_premises enable row level security;
alter table degine.labels enable row level security;
alter table degine.asserts enable row level security;
alter table degine.comments enable row level security;
alter table degine.graphs enable row level security;
alter table degine.assert_graphs enable row level security;
alter table degine.workspaces enable row level security;
alter table degine.workspace_members enable row level security;
alter table degine.profiles enable row level security;
alter table degine.settings enable row level security;

drop policy if exists read_member on degine.facts;
drop policy if exists read_member on degine.fact_citations;
drop policy if exists read_member on degine.rules;
drop policy if exists read_member on degine.rule_premises;
drop policy if exists read_member on degine.labels;
drop policy if exists read_member on degine.asserts;
drop policy if exists read_member on degine.comments;
drop policy if exists read_member on degine.graphs;
drop policy if exists read_member on degine.assert_graphs;
drop policy if exists read_member on degine.workspaces;
drop policy if exists read_member on degine.workspace_members;
drop policy if exists read_profile on degine.profiles;

create policy read_member on degine.facts
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.fact_citations
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.rules
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.rule_premises
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.labels
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.asserts
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.comments
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.graphs
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.assert_graphs
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_member on degine.workspaces
  for select to authenticated
  using ((select degine_private.is_member(id)));
create policy read_member on degine.workspace_members
  for select to authenticated
  using ((select degine_private.is_member(workspace_id)));
create policy read_profile on degine.profiles
  for select to authenticated
  using (
    user_id = (select auth.uid())::text
    or (select degine_private.shares_workspace(user_id))
  );

create index if not exists workspace_members_user_idx on degine.workspace_members (user_id);

-- Deletes need the full row so workspace_id is still there for row level
-- security. Comments keep a single-column primary key, so the default
-- replica identity would hide those deletes.
do $$
declare
  t text;
begin
  foreach t in array array[
    'facts', 'fact_citations', 'rules', 'rule_premises', 'labels', 'asserts',
    'comments', 'graphs', 'assert_graphs', 'workspaces', 'workspace_members'
  ]
  loop
    execute format('alter table degine.%I replica identity full', t);
    if not exists (
      select 1 from pg_publication_tables
      where pubname = 'supabase_realtime' and schemaname = 'degine' and tablename = t
    ) then
      execute format('alter publication supabase_realtime add table degine.%I', t);
    end if;
  end loop;
end $$;

-- Presence and library changes are private channels. Postgres change rows are
-- still filtered by each table's own policy. Broadcast is not allowed.
-- Do not alter realtime.messages. Policies on it are allowed. An alter is not.
drop policy if exists degine_realtime_read on realtime.messages;
drop policy if exists degine_presence_write on realtime.messages;

create policy degine_realtime_read on realtime.messages
  for select to authenticated
  using (
    realtime.messages.extension in ('presence', 'postgres_changes')
    and (select degine_private.is_member(
      substring((select realtime.topic()) from '^(?:library|workspace):([A-Za-z0-9_]+)$')
    ))
  );

create policy degine_presence_write on realtime.messages
  for insert to authenticated
  with check (
    realtime.messages.extension = 'presence'
    and (select degine_private.is_member(
      substring((select realtime.topic()) from '^workspace:([A-Za-z0-9_]+)$')
    ))
  );

do $$
begin
  execute $role$alter role authenticator set pgrst.db_schemas = 'public, storage, graphql_public, degine'$role$;
exception
  when insufficient_privilege then
    raise notice 'could not set pgrst.db_schemas. add degine under exposed schemas in the dashboard.';
end $$;

notify pgrst, 'reload config';
notify pgrst, 'reload schema';
