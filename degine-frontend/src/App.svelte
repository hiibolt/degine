<script>
  import { api, ApiError } from "./api.js";
  import { supabase } from "./session.js";
  import {
    applyRealtime,
    dropAssert,
    dropComment,
    dropFact,
    dropLabel,
    dropLabelWhere,
    dropRule,
    emptyBag,
    facesFrom,
    listWorkspaces,
    loadLibrary,
    markPending,
    profileNames,
    project,
    putAssert,
    putComment,
    putFact,
    putLabel,
    putRule,
    watchLibrary,
    watchPresence,
  } from "./live.js";
  import { argumentPieces, renderAssert } from "./export.js";
  import { downloadText, matchDecl, relevantPieces, renderArgument, shortDiagnostic } from "./prose.js";
  import { pushToast } from "./toast.svelte.js";
  import Icon from "./Icon.svelte";
  import Asserts from "./Asserts.svelte";
  import Inbox from "./Inbox.svelte";
  import Library from "./Library.svelte";
  import Outcomes from "./Outcomes.svelte";
  import People from "./People.svelte";
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
  let loading = $state(true);
  let live = $state("off");
  let busy = $state(false);
  let people = $state([]);
  let faces = $state([]);
  let workspaces = $state([]);
  let workspaceId = $state(localStorage.getItem("degine.workspace") || "");
  let userId = $state("");
  let menuOpen = $state(false);
  let freshName = $state("");
  let settings = $state(false);
  let settingsTab = $state("collaborators");
  let members = $state([]);
  let wsName = $state("");
  let inviteName = $state("");
  let deleteArmed = $state(false);
  let reload = $state(0);
  let ticket = 0;
  let personal = $state([]);
  let personId = $state("");
  let personView = $state(null);
  let personViews = $state({});
  let assertPeople = $state(readAssertPeople());
  let bag = emptyBag();
  let booted = "";
  let latestPresence = null;
  let presenceRoom = null;
  const cursor = { tab: "outcomes", kind: "", item: "", title: "" };

  function readAssertPeople() {
    try {
      const saved = JSON.parse(localStorage.getItem("degine.assertPeople") || "{}");
      return saved && typeof saved === "object" ? saved : {};
    } catch {
      return {};
    }
  }

  function personFor(assertId) {
    return assertPeople[assertId] || "";
  }

  function saveAssertPerson(assertId, person) {
    assertPeople = { ...assertPeople, [assertId]: person };
    localStorage.setItem("degine.assertPeople", JSON.stringify(assertPeople));
  }

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
    faces = [];
    bag = emptyBag();
    booted = "";
    workspaces = [];
    members = [];
    settings = false;
    menuOpen = false;
    userId = "";
  }

  const workspace = $derived(workspaces.find((item) => item.id === workspaceId) || null);
  const canWrite = $derived(Boolean(workspace && (workspace.role === "editor" || workspace.creator)));
  const creator = $derived(Boolean(workspace?.creator));
  const inbox = $derived(
    Object.values(comments)
      .flat()
      .filter((comment) => !comment.resolved && comment.author !== username)
      .sort((a, b) => b.id - a.id),
  );

  function lib(path) {
    return `/workspaces/${encodeURIComponent(workspaceId)}${path}`;
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
    const theorem = await api(lib(`/facts/${encodeURIComponent(id)}/derive`), {
      method: "POST",
      token,
      body: { id: nextId, claim },
    });
    bag = await loadLibrary(supabase, workspaceId);
    show(false);
    selected = { kind: "fact", id: theorem.id };
    syncCursor();
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
        userId = session.user.id;
        username = session.user.user_metadata?.username || session.user.email || "";
      } else if (token) {
        clearSession();
      }
    });
    return () => data.subscription.unsubscribe();
  });

  function flatComments(map) {
    return Object.values(map).flat();
  }

  function show(notify) {
    const view = project(bag, userId);
    if (notify) {
      for (const fact of view.facts) {
        if (!facts.some((item) => item.id === fact.id)) pushToast(`added ${fact.claim}`);
      }
      for (const item of view.asserts) {
        if (!asserts.some((known) => known.id === item.id)) pushToast(`added ${item.title}`);
      }
      const knownComments = new Set(flatComments(comments).map((comment) => comment.id));
      for (const comment of flatComments(view.comments)) {
        if (!knownComments.has(comment.id) && comment.author !== username) pushToast(`${comment.author} commented`);
      }
      for (const item of view.asserts) {
        const next = view.graphs[item.id];
        if (!next) continue;
        const prev = graphs[item.id] || { status: "pending" };
        if (prev.status === next.status) continue;
        if (next.status === "proved") pushToast(`${item.title} proved`, "ok");
        else if (next.status === "invalid") pushToast(`could not prove ${item.title}`, "bad");
        else if (next.status === "pending") pushToast(`checking ${item.title}`);
      }
    }
    facts = view.facts;
    rules = view.rules;
    asserts = view.asserts;
    labels = view.labels;
    graphs = view.graphs;
    people = view.people;
    personal = view.personal;
    comments = view.comments;
    members = view.members;
    if (view.workspace) {
      workspaces = workspaces.map((item) =>
        item.id === view.workspace.id
          ? { ...item, name: view.workspace.name, role: item.id === workspaceId ? view.role : item.role, creator: view.workspace.created_by === userId }
          : item,
      );
    }
    if (selected?.kind === "fact" && !view.facts.some((item) => item.id === selected.id)) selected = null;
    if (selected?.kind === "rule" && !view.rules.some((item) => item.id === selected.id)) selected = null;
    if (selected?.kind === "assert" && !view.asserts.some((item) => item.id === selected.id)) selected = null;
    paintFaces();
  }

  function paintFaces() {
    faces = facesFrom(latestPresence, userId, members);
  }

  function cursorBody() {
    return {
      user_id: userId,
      username,
      tab: cursor.tab,
      kind: cursor.kind,
      item: cursor.item,
      title: cursor.title,
    };
  }

  function syncCursor() {
    cursor.tab = tab;
    const showItem = (tab === "library" || tab === "asserts") && selected;
    cursor.kind = showItem ? selected.kind : "";
    cursor.item = showItem ? selected.id : "";
    cursor.title = "";
    if (!showItem) return;
    if (selected.kind === "assert") cursor.title = asserts.find((item) => item.id === selected.id)?.title || selected.id;
    else if (selected.kind === "fact") cursor.title = facts.find((item) => item.id === selected.id)?.claim || selected.id;
    else cursor.title = selected.id;
  }

  let trackTimer = 0;
  function scheduleTrack(immediate) {
    clearTimeout(trackTimer);
    const send = () => presenceRoom?.track(cursorBody());
    if (immediate) send();
    else trackTimer = setTimeout(send, 6000);
  }

  function setTab(next) {
    tab = next;
    syncCursor();
    scheduleTrack(false);
  }

  function inputsOf(assertId, personId) {
    const assert = asserts.find((item) => item.id === assertId);
    const person = people.find((item) => item.id === personId);
    return JSON.stringify({
      formula: assert?.formula || "",
      assumes: [...(assert?.assumes || [])].sort(),
      on: [...(person?.on || [])].sort(),
      facts: facts.map((fact) => `${fact.id}|${fact.role}|${fact.formula || ""}|${fact.claim}`).sort(),
      personal: personal.map((item) => `${item.id}|${item.claim}`).sort(),
    });
  }

  async function checkPerson(assertId, person) {
    const fp = inputsOf(assertId, person);
    const prev = personViews[assertId];
    const fresh = prev?.personId === person && prev.fp === fp;
    const showing = selected?.kind === "assert" && selected.id === assertId;
    if (fresh) {
      if (showing) personView = prev.view;
      return;
    }
    if (showing) {
      personView = { status: "pending", graph: null, diagnostics: null, missing: [], facts: [] };
    }
    const view = await api(lib(`/asserts/${encodeURIComponent(assertId)}/for/${encodeURIComponent(person)}`), {
      token,
    });
    personViews = { ...personViews, [assertId]: { personId: person, fp, view } };
    if (selected?.kind === "assert" && selected.id === assertId && personFor(assertId) === person) {
      personView = view;
    }
  }

  async function loadAll(current, wanted, who) {
    const mine = ++ticket;
    loading = true;
    try {
      let list = await listWorkspaces(supabase, who);
      if (mine !== ticket) return;
      if (!list.length) {
        const created = await api("/workspaces", { method: "POST", token: current, body: { name: "library" } });
        if (mine !== ticket) return;
        list = [created];
      }
      workspaces = list;
      const id = list.some((item) => item.id === wanted) ? wanted : list[0]?.id || "";
      if (id !== wanted) {
        workspaceId = id;
        if (id) localStorage.setItem("degine.workspace", id);
        return;
      }
      if (!id) return;
      bag = await loadLibrary(supabase, id);
      if (mine !== ticket) return;
      booted = id;
      show(false);
      await Promise.all(
        asserts.map(async (item) => {
          const person = assertPeople[item.id];
          if (person) await checkPerson(item.id, person);
        }),
      );
    } finally {
      if (mine === ticket) loading = false;
    }
  }

  $effect(() => {
    if (!token || !userId || !configured) return;
    const current = token;
    const wanted = workspaceId;
    const who = userId;
    void reload;
    let alive = true;
    loadAll(current, wanted, who).catch((err) => {
      if (alive) handleError(err);
    });
    return () => {
      alive = false;
    };
  });

  $effect(() => {
    if (!token || !userId || !workspaceId || !configured) return;
    const here = workspaceId;
    const who = userId;
    let alive = true;
    const stop = watchLibrary(
      supabase,
      here,
      who,
      async (table, payload) => {
        if (!alive || here !== workspaceId || booted !== here) return;
        applyRealtime(bag, table, payload);
        if (table === "workspace_members" && payload.eventType !== "DELETE" && payload.new?.user_id) {
          const id = payload.new.user_id;
          if (!bag.profiles.some((item) => item.user_id === id)) {
            const found = await profileNames(supabase, [id]);
            if (!alive) return;
            for (const profile of found) {
              if (!bag.profiles.some((item) => item.user_id === profile.user_id)) bag.profiles.push(profile);
            }
          }
        }
        show(true);
      },
      () => {
        if (alive) reload += 1;
      },
      (status) => {
        if (alive) live = status;
      },
    );
    const room = watchPresence(supabase, here, who, cursorBody, (state) => {
      if (!alive) return;
      latestPresence = state;
      paintFaces();
    });
    presenceRoom = room;
    syncCursor();
    room.track(cursorBody());
    const onFocus = () => {
      if (alive) reload += 1;
    };
    window.addEventListener("focus", onFocus);
    return () => {
      alive = false;
      presenceRoom = null;
      latestPresence = null;
      faces = [];
      clearTimeout(trackTimer);
      stop();
      room.stop();
      window.removeEventListener("focus", onFocus);
    };
  });

  $effect(() => {
    void members;
    paintFaces();
  });

  $effect(() => {
    if (!token || !workspaceId || selected?.kind !== "assert") return;
    const person = personFor(selected.id);
    if (!person) return;
    const id = selected.id;
    void facts;
    void people;
    void personal;
    void asserts;
    checkPerson(id, person).catch(handleError);
  });

  function choose(kind, id) {
    drafting = null;
    selected = { kind, id };
    error = "";
    if (kind === "assert") {
      const person = personFor(id);
      if (person) {
        const hit = personViews[id];
        const fp = inputsOf(id, person);
        personView =
          hit?.personId === person && hit.fp === fp
            ? hit.view
            : { status: "pending", graph: null, diagnostics: null, missing: [], facts: [] };
      } else {
        personView = null;
      }
      if (person) checkPerson(id, person).catch(handleError);
    }
    syncCursor();
    scheduleTrack(false);
  }

  function startDraft(kind) {
    selected = null;
    drafting = kind;
    error = "";
    syncCursor();
    scheduleTrack(false);
  }

  async function remember(map) {
    if (!map || !Object.keys(map).length) return;
    await api(lib("/labels"), { method: "PUT", token, body: map });
    for (const [id, title] of Object.entries(map)) putLabel(bag, id, title);
    show(false);
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
      const fact = await api(lib(`/facts/${encodeURIComponent(existingId)}`), {
        method: "PUT",
        token,
        body,
      });
      putFact(bag, fact);
      show(false);
      return;
    }
    const fact = await api(lib("/facts"), { method: "POST", token, body: { id: payload.id, ...body } });
    putFact(bag, fact);
    show(false);
    drafting = null;
    selected = { kind: "fact", id: fact.id };
  }

  async function saveRule(body, existingId) {
    error = "";
    if (existingId) {
      const rule = await api(lib(`/rules/${encodeURIComponent(existingId)}`), {
        method: "PUT",
        token,
        body,
      });
      putRule(bag, rule);
      show(false);
      return;
    }
    const rule = await api(lib("/rules"), { method: "POST", token, body });
    putRule(bag, rule);
    markPending(bag, "rule", rule.id);
    show(false);
    drafting = null;
    selected = { kind: "rule", id: rule.id };
    syncCursor();
    scheduleTrack(false);
  }

  async function removeSelected() {
    if (!selected) return;
    const path = selected.kind === "fact" ? "facts" : selected.kind === "assert" ? "asserts" : "rules";
    await api(lib(`/${path}/${encodeURIComponent(selected.id)}`), { method: "DELETE", token });
    if (selected.kind === "fact") dropFact(bag, selected.id);
    else if (selected.kind === "assert") dropAssert(bag, selected.id);
    else dropRule(bag, selected.id);
    show(false);
  }

  async function saveAssert(payload, existingId) {
    error = "";
    await remember(payload.labels);
    const body = {
      title: payload.title,
      description: payload.description,
      formula: payload.formula,
      assumes: payload.assumes || [],
    };
    if (existingId) {
      markPending(bag, "assert", existingId);
      show(false);
      if (personFor(existingId)) {
        personView = { status: "pending", graph: null, diagnostics: null, missing: [], facts: [] };
      }
      const item = await api(lib(`/asserts/${encodeURIComponent(existingId)}`), {
        method: "PUT",
        token,
        body,
      });
      putAssert(bag, item);
      show(false);
      const person = personFor(existingId);
      if (person) await checkPerson(existingId, person);
      return;
    }
    if (!payload.id) throw new Error("give the title a letter or a number");
    const item = await api(lib("/asserts"), { method: "POST", token, body: { id: payload.id, ...body } });
    putAssert(bag, item);
    markPending(bag, "assert", item.id);
    show(false);
    drafting = null;
    selected = { kind: "assert", id: item.id };
    syncCursor();
    scheduleTrack(false);
  }

  async function addComment(targetType, targetId, body) {
    const comment = await api(lib("/comments"), {
      method: "POST",
      token,
      body: { target_type: targetType, target_id: targetId, body },
    });
    putComment(bag, comment);
    show(false);
  }

  async function editComment(id, body) {
    const comment = await api(lib(`/comments/${id}`), { method: "PUT", token, body: { body } });
    putComment(bag, comment);
    show(false);
  }

  async function removeComment(id) {
    await api(lib(`/comments/${id}`), { method: "DELETE", token });
    dropComment(bag, id);
    show(false);
  }

  async function resolveComment(id, resolved) {
    const comment = await api(lib(`/comments/${id}/resolved`), {
      method: "POST",
      token,
      body: { resolved },
    });
    putComment(bag, comment);
    show(false);
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
    const person = personFor(item.id);
    const cached = personViews[item.id];
    const record = person && cached?.personId === person ? cached.view : graphs[item.id];
    if (record?.status !== "proved") return;
    busy = true;
    error = "";
    try {
      const used = argumentPieces(item, facts);
      const targets = [["assert", item.id], ...used.map((fact) => ["fact", fact.id])];
      const threads = targets.map(([type, id]) => comments[`${type}:${id}`] || []);
      downloadText(
        `${item.title.replace(/[^\w.-]+/g, "_")}.md`,
        renderAssert({
          assert: item,
          facts,
          labels,
          record,
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
      const threads = targets.map(([type, id]) => comments[`${type}:${id}`] || []);
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

  function pickWorkspace(id) {
    menuOpen = false;
    if (id === workspaceId) return;
    selected = null;
    drafting = null;
    loading = true;
    facts = [];
    rules = [];
    asserts = [];
    labels = {};
    graphs = {};
    comments = {};
    people = [];
    bag = emptyBag();
    booted = "";
    personal = [];
    personView = null;
    personViews = {};
    workspaceId = id;
    localStorage.setItem("degine.workspace", id);
  }

  async function makeWorkspace(event) {
    event.preventDefault();
    const name = freshName.trim();
    if (!name) return;
    try {
      const created = await api("/workspaces", { method: "POST", token, body: { name } });
      freshName = "";
      pickWorkspace(created.id);
    } catch (err) {
      handleError(err);
    }
  }

  async function loadMembers() {
    const next = await loadLibrary(supabase, workspaceId);
    bag.members = next.members;
    bag.profiles = next.profiles;
    if (next.workspace) bag.workspace = next.workspace;
    show(false);
  }

  function openSettings() {
    menuOpen = false;
    settings = true;
    settingsTab = "collaborators";
    deleteArmed = false;
    wsName = workspace?.name || "";
    inviteName = "";
    loadMembers().catch(handleError);
  }

  async function renameWorkspace(event) {
    event.preventDefault();
    const name = wsName.trim();
    if (!name || !workspace) return;
    try {
      await api(`/workspaces/${encodeURIComponent(workspace.id)}`, { method: "PUT", token, body: { name } });
      if (bag.workspace?.id === workspace.id) bag.workspace = { ...bag.workspace, name };
      show(false);
    } catch (err) {
      handleError(err);
    }
  }

  async function deleteWorkspace() {
    if (!deleteArmed) {
      deleteArmed = true;
      return;
    }
    try {
      await api(`/workspaces/${encodeURIComponent(workspaceId)}`, { method: "DELETE", token });
      settings = false;
      localStorage.removeItem("degine.workspace");
      selected = null;
      workspaceId = "";
      reload += 1;
    } catch (err) {
      handleError(err);
    }
  }

  async function invite(event) {
    event.preventDefault();
    const name = inviteName.trim().toLowerCase();
    if (!name) return;
    try {
      await api(lib("/members"), { method: "POST", token, body: { username: name } });
      inviteName = "";
      await loadMembers();
    } catch (err) {
      handleError(err);
    }
  }

  async function setRole(memberId, role) {
    try {
      await api(lib(`/members/${encodeURIComponent(memberId)}`), { method: "PUT", token, body: { role } });
      await loadMembers();
    } catch (err) {
      handleError(err);
    }
  }

  async function removeMember(memberId) {
    try {
      await api(lib(`/members/${encodeURIComponent(memberId)}`), { method: "DELETE", token });
      await loadMembers();
    } catch (err) {
      handleError(err);
    }
  }

  async function leaveWorkspace() {
    try {
      await api(lib("/leave"), { method: "POST", token });
      settings = false;
      localStorage.removeItem("degine.workspace");
      selected = null;
      workspaceId = "";
      reload += 1;
    } catch (err) {
      handleError(err);
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
          <button class="tab" class:on={tab === "library"} type="button" onclick={() => setTab("library")}>
            library
          </button>
          <button class="tab" class:on={tab === "asserts"} type="button" onclick={() => setTab("asserts")}>
            asserts
          </button>
          <button class="tab" class:on={tab === "outcomes"} type="button" onclick={() => setTab("outcomes")}>
            outcomes
          </button>
          <button class="tab" class:on={tab === "people"} type="button" onclick={() => setTab("people")}>
            people
          </button>
        </nav>
      </div>
      <div class="hint session">
        {#if faces.length}
          <div class="faces">
            {#each faces as face (face.user_id)}
              <span class="face" style:background={face.color} title={face.label}>{face.initials}</span>
            {/each}
          </div>
        {/if}
        <button class="quiet" type="button" onclick={openAccount}>{username}</button>
        <div class="ws">
          <button class="ws-btn" type="button" onclick={() => (menuOpen = !menuOpen)}>
            <span>{workspace?.name || "workspace"}</span>
          </button>
          <button class="icon" type="button" aria-label="workspace settings" title="settings" onclick={openSettings}>
            <Icon name="gear" />
          </button>
          {#if menuOpen}
            <div class="menu">
              {#each workspaces as item (item.id)}
                <button class="quiet" type="button" onclick={() => pickWorkspace(item.id)}>
                  {item.name}{item.id === workspaceId ? " · here" : ""}
                </button>
              {/each}
              <form class="share-add" onsubmit={makeWorkspace}>
                <input bind:value={freshName} placeholder="new workspace" autocomplete="off" />
                <button class="quiet" type="submit">new</button>
              </form>
            </div>
          {/if}
        </div>
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
      <div class="shell skel" aria-busy="true">
        <aside class="rail">
          {#each [0, 1, 2, 3, 4, 5, 6] as i (i)}
            <div class="skel-row" style:animation-delay="{i * 70}ms"></div>
          {/each}
        </aside>
        <div class="detail skel-detail">
          <div class="skel-line short"></div>
          <div class="skel-line"></div>
          <div class="skel-block"></div>
        </div>
      </div>
    {:else}
    <div class="arrive">
    {#if tab === "outcomes"}
      <Outcomes
        {facts}
        {labels}
        {asserts}
        {personal}
        {people}
        {personId}
        onperson={(id) => {
          personId = id;
          personView = null;
        }}
        {canWrite}
        onimport={async (payload) => {
          await saveAssert(payload);
          setTab("asserts");
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
        {canWrite}
        {username}
        {comments}
        oncomment={addComment}
        oneditcomment={editComment}
        ondeletecomment={removeComment}
        onresolve={resolveComment}
        onexport={exportAssert}
        {people}
        {assertPeople}
        personId={selected?.kind === "assert" ? personFor(selected.id) : ""}
        {personViews}
        {personView}
        onperson={async (id) => {
          if (selected?.kind !== "assert") return;
          saveAssertPerson(selected.id, id);
          personView = null;
          if (id) await checkPerson(selected.id, id);
        }}
        onopenFact={(id) => {
          tab = "library";
          choose("fact", id);
        }}
      />
    {:else if tab === "people"}
      <People
        {people}
        {personal}
        {canWrite}
        onerror={handleError}
        onaddPerson={async (body) => {
          const person = await api(lib("/people"), { token, method: "POST", body });
          putLabel(bag, `~person:${person.id}`, person.name);
          show(false);
        }}
        onremovePerson={async (id) => {
          await api(lib(`/people/${encodeURIComponent(id)}`), { token, method: "DELETE" });
          dropLabelWhere(bag, (label) => label === `~person:${id}` || label.startsWith(`~on:${id}:`));
          if (personId === id) {
            personId = "";
            personView = null;
          }
          show(false);
        }}
        onaddFact={async (body) => {
          const fact = await api(lib("/personal-facts"), { token, method: "POST", body });
          putLabel(bag, `~pfact:${fact.id}`, fact.claim);
          show(false);
        }}
        ontoggle={async (person, fact, on) => {
          await api(lib(`/people/${encodeURIComponent(person)}/toggles/${encodeURIComponent(fact)}`), {
            token,
            method: "PUT",
            body: { on },
          });
          const id = `~on:${person}:${fact}`;
          if (on) putLabel(bag, id, "on");
          else dropLabel(bag, id);
          show(false);
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
      {canWrite}
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
    {#if settings && workspace}
      <div class="veil" onclick={() => (settings = false)}>
        <div class="card sheet" onclick={(event) => event.stopPropagation()}>
          <nav class="side">
            <button class="quiet" class:on={settingsTab === "general"} type="button" onclick={() => (settingsTab = "general")}>general</button>
            <button class="quiet" class:on={settingsTab === "collaborators"} type="button" onclick={() => (settingsTab = "collaborators")}>collaborators</button>
          </nav>
          {#if settingsTab === "general"}
            <form class="stack" onsubmit={renameWorkspace}>
              <h2>general</h2>
              <label>
                name
                <input bind:value={wsName} disabled={!creator} />
              </label>
              {#if creator}
                <button class="primary" type="submit">save name</button>
              {/if}
              <div class="scary">
                {#if creator}
                  <p class="dek">Deleting removes the library, the people, and every share of this workspace.</p>
                  <button class="danger" type="button" onclick={deleteWorkspace}>
                    {deleteArmed ? "yes, delete it" : "delete workspace"}
                  </button>
                {:else}
                  <p class="dek">You can leave. The person who made it stays.</p>
                  <button class="danger" type="button" onclick={leaveWorkspace}>leave workspace</button>
                {/if}
              </div>
            </form>
          {:else}
            <div class="stack">
              <h2>collaborators</h2>
              {#each members as member (member.user_id)}
                <div class="member-row">
                  <span>{member.username || "someone"}{member.creator ? " · creator" : ""}</span>
                  {#if creator && !member.creator}
                    <select value={member.role} onchange={(event) => setRole(member.user_id, event.currentTarget.value)}>
                      <option value="reader">reader</option>
                      <option value="editor">editor</option>
                    </select>
                    <button class="quiet" type="button" onclick={() => removeMember(member.user_id)}>remove</button>
                  {:else}
                    <span class="empty">{member.creator ? "editor" : member.role}</span>
                  {/if}
                </div>
              {/each}
              {#if creator}
                <form class="share-add" onsubmit={invite}>
                  <input bind:value={inviteName} placeholder="username" autocomplete="off" />
                  <button class="quiet" type="submit">invite</button>
                </form>
                <p class="dek">Invites start as readers. They need an account already.</p>
              {:else if members.some((member) => member.user_id === userId)}
                <button class="quiet" type="button" onclick={leaveWorkspace}>leave</button>
              {/if}
            </div>
          {/if}
        </div>
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
