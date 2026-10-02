<script>
  import { api, connectEvents, ApiError } from "./api.js";
  import { supabase } from "./session.js";
  import { argumentPieces, renderAssert } from "./export.js";
  import { downloadText, matchDecl, relevantPieces, renderArgument, shortDiagnostic } from "./prose.js";
  import { pushToast } from "./toast.svelte.js";
  import Asserts from "./Asserts.svelte";
  import Inbox from "./Inbox.svelte";
  import Library from "./Library.svelte";
  import Outcomes from "./Outcomes.svelte";
  import Toast from "./Toast.svelte";

  let authMode = $state("in");
  let signEmail = $state("");
  let signPassword = $state("");
  let joinEmail = $state("");
  let joinName = $state("");
  let joinPassword = $state("");
  let joinConfirm = $state("");
  let token = $state("");
  let username = $state("");
  let account = $state(false);
  let apiToken = $state("");
  let facts = $state([]);
  let rules = $state([]);
  let asserts = $state([]);
  let labels = $state({});
  let graphs = $state({});
  let tab = $state("outcomes");
  let comments = $state({});
  let selected = $state(null);
  let drafting = $state(null);
  let error = $state("");
  let loading = $state(false);
  let live = $state("off");
  let busy = $state(false);
  let owner = $state(false);
  let inbox = $state([]);
  let shares = $state([]);

  const configured = Boolean(supabase);

  function handleError(err) {
    if (err instanceof ApiError && err.status === 401 && token) {
      logout();
      error = "the session ended. log in again.";
      return;
    }
    error = err?.message || "something went wrong";
  }

  function clearSession() {
    token = "";
    signPassword = "";
    username = "";
    facts = [];
    rules = [];
    asserts = [];
    labels = {};
    graphs = {};
    tab = "library";
    comments = {};
    selected = null;
    drafting = null;
    live = "off";
    owner = false;
    inbox = [];
    shares = [];
  }

  async function logout() {
    clearSession();
    await supabase?.auth.signOut();
  }

  function cleanName(value) {
    return value.trim().toLowerCase();
  }

  async function openAccount() {
    account = true;
    error = "";
    try {
      const body = await api("/account/token", { token });
      apiToken = body.token;
    } catch (err) {
      handleError(err);
    }
  }

  async function copyToken() {
    await navigator.clipboard.writeText(apiToken);
  }

  async function resetToken() {
    const body = await api("/account/token", { method: "POST", token });
    apiToken = body.token;
  }

  async function deriveFact(id, nextId, claim) {
    const theorem = await api(`/facts/${encodeURIComponent(id)}/derive`, {
      method: "POST",
      token,
      body: { id: nextId, claim },
    });
    await loadAll(token);
    selected = { kind: "fact", id: theorem.id };
    return theorem;
  }

  async function login(event) {
    event.preventDefault();
    error = "";
    const email = signEmail.trim().toLowerCase();
    if (!email.includes("@") || !signPassword) {
      error = "email and password";
      return;
    }
    try {
      const { error: authError } = await supabase.auth.signInWithPassword({
        email,
        password: signPassword,
      });
      if (authError) throw new Error(authError.message);
      signPassword = "";
    } catch (err) {
      handleError(err);
    }
  }

  async function signup(event) {
    event.preventDefault();
    error = "";
    const email = joinEmail.trim().toLowerCase();
    const name = cleanName(joinName);
    if (!email.includes("@") || !name || !joinPassword) {
      error = "email, username, and password";
      return;
    }
    if (!/^[a-z0-9_]{3,32}$/.test(name)) {
      error = "username is 3 to 32 letters, numbers, or underscores";
      return;
    }
    if (joinPassword !== joinConfirm) {
      error = "passwords don't match";
      return;
    }
    try {
      const { data, error: authError } = await supabase.auth.signUp({
        email,
        password: joinPassword,
        options: { data: { username: name } },
      });
      if (authError) throw new Error(authError.message);
      joinPassword = "";
      joinConfirm = "";
      if (!data.session) error = "check your email to finish signing up.";
    } catch (err) {
      handleError(err);
    }
  }

  $effect(() => {
    if (!supabase) return;
    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      if (session) {
        token = session.access_token;
        username = session.user.user_metadata?.username || session.user.email || "";
      } else if (token) {
        clearSession();
      }
    });
    return () => data.subscription.unsubscribe();
  });

  function upsert(list, item) {
    const next = list.filter((entry) => entry.id !== item.id);
    next.push(item);
    next.sort((a, b) => a.id.localeCompare(b.id));
    return next;
  }

  function titleOfAssert(id) {
    return asserts.find((item) => item.id === id)?.title || "";
  }

  function putComment(comment) {
    const key = `${comment.target_type}:${comment.target_id}`;
    const thread = comments[key] || [];
    const next = thread.some((item) => item.id === comment.id)
      ? thread.map((item) => (item.id === comment.id ? comment : item))
      : [...thread, comment];
    comments = { ...comments, [key]: next };
    const rest = inbox.filter((item) => item.id !== comment.id);
    inbox = comment.author !== username && !comment.resolved ? [comment, ...rest] : rest;
  }

  function apply(event, notify = false) {
    if (event.type === "fact_changed") {
      const known = facts.some((fact) => fact.id === event.fact.id);
      const fact = owner ? event.fact : { ...event.fact, shared: true };
      facts = upsert(facts, fact);
      if (notify && !known) pushToast(`added ${fact.claim}`);
    } else if (event.type === "fact_deleted") {
      facts = facts.filter((fact) => fact.id !== event.id);
      if (selected?.kind === "fact" && selected.id === event.id) selected = null;
    } else if (event.type === "rule_changed") rules = upsert(rules, event.rule);
    else if (event.type === "rule_deleted") {
      rules = rules.filter((rule) => rule.id !== event.id);
      const next = { ...graphs };
      delete next[event.id];
      graphs = next;
      if (selected?.kind === "rule" && selected.id === event.id) selected = null;
    } else if (event.type === "assert_changed") {
      const known = asserts.some((item) => item.id === event.assert.id);
      const item = owner ? event.assert : { ...event.assert, shared: true };
      asserts = upsert(asserts, item);
      if (notify && !known) pushToast(`added ${item.title}`);
    } else if (event.type === "assert_deleted") {
      asserts = asserts.filter((item) => item.id !== event.id);
      const next = { ...graphs };
      delete next[event.id];
      graphs = next;
      if (selected?.kind === "assert" && selected.id === event.id) selected = null;
    } else if (event.type === "comment_added" || event.type === "comment_changed") {
      putComment(event.comment);
      if (notify && event.type === "comment_added" && event.comment.author !== username) {
        pushToast(`${event.comment.author} commented`);
      }
    } else if (event.type === "comment_deleted") {
      const key = `${event.target_type}:${event.target_id}`;
      comments = {
        ...comments,
        [key]: (comments[key] || []).filter((comment) => comment.id !== event.id),
      };
      inbox = inbox.filter((comment) => comment.id !== event.id);
    } else if (event.type === "compile_started") {
      const title = titleOfAssert(event.target_rule_id);
      const prev = graphs[event.target_rule_id];
      graphs = {
        ...graphs,
        [event.target_rule_id]: { graph: null, diagnostics: null, ...prev, status: "pending" },
      };
      if (notify && title && prev?.status !== "pending") pushToast(`checking ${title}`);
    } else if (event.type === "graph_updated") {
      const title = titleOfAssert(event.target_rule_id);
      graphs = {
        ...graphs,
        [event.target_rule_id]: { status: "proved", graph: event.graph, diagnostics: null },
      };
      if (notify && title) pushToast(`${title} proved`, "ok");
    } else if (event.type === "compile_failed") {
      const title = titleOfAssert(event.target_rule_id);
      graphs = {
        ...graphs,
        [event.target_rule_id]: {
          status: "invalid",
          graph: null,
          diagnostics: event.diagnostics,
        },
      };
      if (notify && title) pushToast(`could not prove ${title}`, "bad");
    }
  }

  async function loadAll(current) {
    loading = true;
    try {
    const me = await api("/me", { token: current });
    owner = me.owner;
    const [nextFacts, nextRules, nextAsserts, nextLabels, nextInbox] = await Promise.all([
      api("/facts", { token: current }),
      api("/rules", { token: current }),
      api("/asserts", { token: current }),
      api("/labels", { token: current }),
      api("/inbox", { token: current }),
    ]);
    inbox = nextInbox;
    facts = nextFacts;
    rules = nextRules;
    asserts = nextAsserts;
    labels = nextLabels;
    const loaded = await Promise.all(
      [...nextRules.map((rule) => ["rules", rule.id]), ...nextAsserts.map((item) => ["asserts", item.id])].map(
        async ([kind, id]) => [id, await api(`/${kind}/${encodeURIComponent(id)}/graph`, { token: current })],
      ),
    );
    graphs = Object.fromEntries(loaded);
    if (selected) await loadThreads(current, selected.kind, selected.id);
    } finally {
      loading = false;
    }
  }

  async function loadThreads(current, kind, id) {
    const types = kind === "assert" ? ["assert"] : kind === "rule" ? ["rule", "conclusion"] : ["fact"];
    const entries = await Promise.all(
      types.map(async (type) => {
        const thread = await api(
          `/comments?target_type=${encodeURIComponent(type)}&target_id=${encodeURIComponent(id)}`,
          { token: current },
        );
        return [`${type}:${id}`, thread];
      }),
    );
    comments = { ...comments, ...Object.fromEntries(entries) };
  }

  $effect(() => {
    if (!token || !configured) return;
    const current = token;
    let alive = true;
    loadAll(current).catch((err) => {
      if (alive) handleError(err);
    });
    const stop = connectEvents(
      current,
      (event) => {
        if (!alive) return;
        if (event.type === "reconnected") loadAll(current).catch((err) => alive && handleError(err));
        else if (event.type === "access_changed") loadAll(current).catch((err) => alive && handleError(err));
        else apply(event, true);
      },
      (status) => {
        if (alive) live = status;
      },
    );
    return () => {
      alive = false;
      stop();
    };
  });

  function choose(kind, id) {
    drafting = null;
    selected = { kind, id };
    error = "";
    loadThreads(token, kind, id).catch(handleError);
    if (kind === "assert") {
      shares = [];
      api(`/asserts/${encodeURIComponent(id)}/graph`, { token })
        .then((record) => {
          graphs = { ...graphs, [id]: record };
        })
        .catch(handleError);
      return;
    }
    if (kind === "rule") {
      api(`/rules/${encodeURIComponent(id)}/graph`, { token })
        .then((record) => {
          graphs = { ...graphs, [id]: record };
        })
        .catch(handleError);
    }
  }

  function startDraft(kind) {
    selected = null;
    drafting = kind;
    error = "";
  }

  async function remember(map) {
    if (!map || !Object.keys(map).length) return;
    await api("/labels", { method: "PUT", token, body: map });
    labels = { ...labels, ...map };
  }

  async function saveFact(payload, existingId) {
    error = "";
    await remember(payload.labels);
    const body = {
      claim: payload.claim,
      citations: payload.citations,
      formula: payload.formula,
      role: payload.role,
    };
    if (existingId) {
      const fact = await api(`/facts/${encodeURIComponent(existingId)}`, {
        method: "PUT",
        token,
        body,
      });
      apply({ type: "fact_changed", fact });
      return;
    }
    const fact = await api("/facts", { method: "POST", token, body: { id: payload.id, ...body } });
    apply({ type: "fact_changed", fact });
    drafting = null;
    selected = { kind: "fact", id: fact.id };
  }

  async function saveRule(body, existingId) {
    error = "";
    if (existingId) {
      const rule = await api(`/rules/${encodeURIComponent(existingId)}`, {
        method: "PUT",
        token,
        body,
      });
      apply({ type: "rule_changed", rule });
      return;
    }
    const rule = await api("/rules", { method: "POST", token, body });
    apply({ type: "rule_changed", rule });
    drafting = null;
    selected = { kind: "rule", id: rule.id };
    const record = await api(`/rules/${encodeURIComponent(rule.id)}/graph`, { token });
    graphs = { ...graphs, [rule.id]: record };
  }

  async function removeSelected() {
    if (!selected) return;
    const path = selected.kind === "fact" ? "facts" : selected.kind === "assert" ? "asserts" : "rules";
    await api(`/${path}/${encodeURIComponent(selected.id)}`, { method: "DELETE", token });
    const deleted =
      selected.kind === "fact"
        ? { type: "fact_deleted", id: selected.id }
        : selected.kind === "assert"
          ? { type: "assert_deleted", id: selected.id }
          : { type: "rule_deleted", id: selected.id };
    apply(deleted);
  }

  async function saveAssert(payload, existingId) {
    error = "";
    await remember(payload.labels);
    const body = {
      title: payload.title,
      description: payload.description,
      formula: payload.formula,
    };
    if (existingId) {
      const item = await api(`/asserts/${encodeURIComponent(existingId)}`, {
        method: "PUT",
        token,
        body,
      });
      apply({ type: "assert_changed", assert: item });
      return;
    }
    if (!payload.id) throw new Error("give the title a letter or a number");
    const item = await api("/asserts", { method: "POST", token, body: { id: payload.id, ...body } });
    apply({ type: "assert_changed", assert: item });
    drafting = null;
    selected = { kind: "assert", id: item.id };
    const record = await api(`/asserts/${encodeURIComponent(item.id)}/graph`, { token });
    graphs = { ...graphs, [item.id]: record };
  }

  async function addComment(targetType, targetId, body) {
    const comment = await api("/comments", {
      method: "POST",
      token,
      body: { target_type: targetType, target_id: targetId, body },
    });
    apply({ type: "comment_added", comment });
  }

  async function editComment(id, body) {
    const comment = await api(`/comments/${id}`, { method: "PUT", token, body: { body } });
    apply({ type: "comment_changed", comment });
  }

  async function removeComment(id) {
    const found = Object.values(comments)
      .flat()
      .find((comment) => comment.id === id);
    await api(`/comments/${id}`, { method: "DELETE", token });
    apply({
      type: "comment_deleted",
      id,
      target_type: found?.target_type || "",
      target_id: found?.target_id || "",
    });
  }

  async function resolveComment(id, resolved) {
    const comment = await api(`/comments/${id}/resolved`, {
      method: "POST",
      token,
      body: { resolved },
    });
    apply({ type: "comment_changed", comment });
  }

  async function loadShares(id) {
    shares = await api(`/asserts/${encodeURIComponent(id)}/shares`, { token });
  }

  async function toggleShare(id, person, on) {
    if (on) {
      await api(`/asserts/${encodeURIComponent(id)}/share`, {
        method: "POST",
        token,
        body: { email: person },
      });
      if (!shares.includes(person)) shares = [...shares, person];
    } else {
      await api(`/asserts/${encodeURIComponent(id)}/share/${encodeURIComponent(person)}`, {
        method: "DELETE",
        token,
      });
      shares = shares.filter((name) => name !== person);
    }
  }

  function openInbox(comment) {
    if (comment.target_type === "assert") {
      tab = "asserts";
      choose("assert", comment.target_id);
    } else if (comment.target_type === "fact") {
      tab = "library";
      choose("fact", comment.target_id);
    }
  }

  async function exportAssert(item) {
    busy = true;
    error = "";
    try {
      const used = argumentPieces(item, facts);
      const targets = [["assert", item.id], ...used.map((fact) => ["fact", fact.id])];
      const threads = await Promise.all(
        targets.map(([type, id]) =>
          api(`/comments?target_type=${encodeURIComponent(type)}&target_id=${encodeURIComponent(id)}`, { token }),
        ),
      );
      downloadText(
        `${item.title.replace(/[^\w.-]+/g, "_")}.md`,
        renderAssert({
          assert: item,
          facts,
          labels,
          record: graphs[item.id],
          comments: threads.flat(),
        }),
      );
    } catch (err) {
      handleError(err);
    } finally {
      busy = false;
    }
  }

  async function exportRule(rule) {
    busy = true;
    error = "";
    try {
      const { facts: usedFacts, rules: usedRules } = relevantPieces(rule, facts, rules);
      const targets = [
        ...usedFacts.map((fact) => ["fact", fact.id]),
        ...usedRules.flatMap((item) => [
          ["rule", item.id],
          ["conclusion", item.id],
        ]),
      ];
      const threads = await Promise.all(
        targets.map(([type, id]) =>
          api(`/comments?target_type=${encodeURIComponent(type)}&target_id=${encodeURIComponent(id)}`, { token }),
        ),
      );
      const text = renderArgument({
        rule,
        facts,
        rules,
        record: graphs[rule.id],
        comments: threads.flat(),
      });
      downloadText(`${rule.id.replace(/[^\w.-]+/g, "_")}.md`, text);
    } catch (err) {
      handleError(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if !configured}
  <main class="page">
    <p class="banner">set the api and supabase env vars, then start the app again.</p>
  </main>
{:else if !token}
  <main class="page">
    {#if authMode === "in"}
      <form class="card login stack" onsubmit={login}>
        <h1>degine</h1>
        <label>
          email
          <input bind:value={signEmail} type="email" autocomplete="email" />
        </label>
        <label>
          password
          <input type="password" bind:value={signPassword} autocomplete="current-password" />
        </label>
        {#if error}<p class="banner">{error}</p>{/if}
        <button class="primary" type="submit">log in</button>
        <button class="quiet" type="button" onclick={() => { authMode = "up"; error = ""; }}>sign up</button>
      </form>
    {:else}
      <form class="card login stack" onsubmit={signup}>
        <h1>degine</h1>
        <label>
          email
          <input bind:value={joinEmail} type="email" autocomplete="email" />
        </label>
        <label>
          username
          <input bind:value={joinName} autocomplete="username" />
        </label>
        <label>
          password
          <input type="password" bind:value={joinPassword} autocomplete="new-password" />
        </label>
        <label>
          confirm password
          <input type="password" bind:value={joinConfirm} autocomplete="new-password" />
        </label>
        {#if error}<p class="banner">{error}</p>{/if}
        <button class="primary" type="submit">sign up</button>
        <button class="quiet" type="button" onclick={() => { authMode = "in"; error = ""; }}>log in</button>
      </form>
    {/if}
  </main>
{:else}
  <main class="page">
    <header class="top">
      <div class="brand">
        <h1>degine</h1>
        <nav class="tabs">
          <button class="tab" class:on={tab === "library"} type="button" onclick={() => (tab = "library")}>
            library
          </button>
          <button class="tab" class:on={tab === "asserts"} type="button" onclick={() => (tab = "asserts")}>
            asserts
          </button>
          <button class="tab" class:on={tab === "outcomes"} type="button" onclick={() => (tab = "outcomes")}>
            outcomes
          </button>
        </nav>
      </div>
      <div class="hint session">
        <button class="quiet" type="button" onclick={openAccount}>{username}</button>
        {#if live === "open"}
          · live
        {:else if live === "connecting" || live === "closed"}
          · reconnecting
        {/if}
        <Inbox items={inbox} onopen={openInbox} />
        <button class="quiet" type="button" onclick={logout}>log out</button>
      </div>
    </header>
    <Toast />
    {#if error}<p class="banner">{error}</p>{/if}
    {#if loading && facts.length === 0}
      <p class="loading">loading</p>
    {:else}
    <div class="arrive">
    {#if tab === "outcomes"}
      <Outcomes
        {facts}
        {labels}
        {asserts}
        {owner}
        onimport={async (payload) => {
          await saveAssert(payload);
          tab = "asserts";
        }}
        onopenFact={(id) => {
          tab = "library";
          choose("fact", id);
        }}
      />
    {:else if tab === "asserts"}
      <Asserts
        {asserts}
        {facts}
        {labels}
        {graphs}
        {selected}
        drafting={drafting === "assert"}
        onsave={saveAssert}
        onremove={removeSelected}
        onchoose={(id) => choose("assert", id)}
        oncreate={() => startDraft("assert")}
        oncancel={() => (drafting = null)}
        onerror={handleError}
        {owner}
        {username}
        {shares}
        {comments}
        oncomment={addComment}
        oneditcomment={editComment}
        ondeletecomment={removeComment}
        onresolve={resolveComment}
        onexport={exportAssert}
        onshares={loadShares}
        ontoggleShare={toggleShare}
        onopenFact={(id) => {
          tab = "library";
          choose("fact", id);
        }}
      />
    {:else}
    <Library
      {facts}
      {labels}
      {asserts}
      {selected}
      drafting={drafting === "fact" || drafting === "criterion" || drafting === "theorem" ? drafting : null}
      onsave={saveFact}
      onremove={removeSelected}
      onchoose={(id) => choose("fact", id)}
      ondraft={(kind) => startDraft(kind)}
      oncancel={() => (drafting = null)}
      onerror={handleError}
      {owner}
      {username}
      {comments}
      oncomment={addComment}
      oneditcomment={editComment}
      ondeletecomment={removeComment}
      onresolve={resolveComment}
      onderive={deriveFact}
      onopenAssert={(id) => {
        tab = "asserts";
        choose("assert", id);
      }}
    />
    {/if}
    </div>
    {/if}
    {#if account}
      <div class="veil" onclick={() => (account = false)}>
        <form class="card login stack" onclick={(event) => event.stopPropagation()} onsubmit={(event) => event.preventDefault()}>
          <h2>account</h2>
          <p class="kicker">{username}</p>
          <label>
            api token
            <input readonly value={apiToken} />
          </label>
          <button class="primary" type="button" onclick={copyToken}>copy</button>
          <button class="quiet" type="button" onclick={resetToken}>regenerate</button>
          <button class="quiet" type="button" onclick={() => (account = false)}>close</button>
        </form>
      </div>
    {/if}
  </main>
{/if}
