<script>
  import { tick } from "svelte";
  import ClaimFields from "./ClaimFields.svelte";
  import ClaimView from "./ClaimView.svelte";
  import Comments from "./Comments.svelte";
  import Icon from "./Icon.svelte";
  import ProofDag from "./ProofDag.svelte";
  import { atomChoices, compileClaim, fillName, readClaim, uniqueSlug } from "./phrases.js";
  import { explainAssert } from "./proof.js";

  let {
    asserts,
    facts,
    labels,
    graphs,
    selected,
    drafting,
    onsave,
    onremove,
    onchoose,
    oncreate,
    oncancel,
    onerror,
    onopenFact,
    canWrite,
    username,
    comments,
    oncomment,
    oneditcomment,
    ondeletecomment,
    onresolve,
    onexport,
    people = [],
    assertPeople = {},
    personId = "",
    personViews = {},
    personView = null,
    onperson,
  } = $props();

  let title = $state("");
  let outcomeId = $state("");
  let enabled = $state([]);
  let legacy = $state(false);
  let description = $state("");
  let join = $state("and");
  let parts = $state(["", ""]);
  let thenId = $state("");
  let editing = $state(false);
  let logs = $state(false);
  let base = $state("");
  let query = $state("");
  let searching = $state(false);
  let finder = $state(null);
  let seen = "";

  function blurb(formula) {
    if (!formula) return "";
    const claim = readClaim(formula);
    if (!claim) return formula;
    const left = claim.partIds.map(nameOf).join(claim.join === "or" ? " or " : " and ");
    return `if ${left}, then ${nameOf(claim.thenId)}`;
  }

  function assertState(assert) {
    const person = assertPeople[assert.id];
    const cached = personViews[assert.id];
    if (person && cached?.personId === person) return cached.view.status;
    return graphs[assert.id]?.status || "pending";
  }

  function assertTitle(assert) {
    const saved = people.find((item) => item.id === assertPeople[assert.id])?.name || "someone";
    return fillName(assert.title, saved);
  }

  function nameOf(id) {
    const raw = viewFacts.find((fact) => fact.id === id && !fact.formula)?.claim || labels[id] || id;
    return fillName(raw, personName || "someone");
  }

  function linesOf(items) {
    return items.map((part) => part.trim()).filter(Boolean);
  }

  const open = $derived(
    !drafting && selected?.kind === "assert" ? asserts.find((item) => item.id === selected.id) : null,
  );
  const personName = $derived(people.find((item) => item.id === personId)?.name || "");
  const record = $derived(personId && personView ? personView : open ? graphs[open.id] : null);
  const viewFacts = $derived(
    personView?.facts?.length ? [...facts.filter((fact) => !personView.facts.some((item) => item.id === fact.id)), ...personView.facts] : facts,
  );
  const status = $derived(record?.status || (open ? "pending" : ""));
  const titleOf = (id) => nameOf(id);
  const tree = $derived(
    open && record?.status === "proved"
      ? explainAssert(open.formula, viewFacts, titleOf, record.graph)
      : null,
  );
  const shown = $derived(open?.formula ? readClaim(open.formula) : null);
  const options = $derived(atomChoices(facts, labels));
  const visible = $derived(
    asserts
      .filter((item) => {
        const needle = query.trim().toLowerCase();
        if (!needle) return true;
        return item.title.toLowerCase().includes(needle) || blurb(item.formula).toLowerCase().includes(needle);
      })
      .sort((a, b) => a.title.localeCompare(b.title)),
  );
  $effect(() => {
    if (drafting || open) return;
    const first = visible[0];
    if (first) onchoose(first.id);
  });
  const thread = $derived(open ? comments[`assert:${open.id}`] || [] : []);
  const forming = $derived(Boolean(drafting) || editing);
  const dirty = $derived(current() !== base);

  function phrase(id) {
    const fact = facts.find((item) => item.id === id && item.role !== "theorem" && !item.formula);
    return { id: fact ? id : "", title: nameOf(id) };
  }

  const outcomes = $derived(
    Object.entries(labels)
      .filter(([id]) => !facts.some((fact) => fact.id === id))
      .map(([id, label]) => ({ id, title: fillName(label), sort: label }))
      .sort((a, b) => a.sort.localeCompare(b.sort)),
  );
  const criteria = $derived(
    facts
      .filter((fact) => fact.role === "criterion" && !fact.formula)
      .map((fact) => ({ id: fact.id, title: fillName(fact.claim), sort: fact.claim }))
      .sort((a, b) => a.sort.localeCompare(b.sort)),
  );

  function current() {
    if (legacy) {
      return JSON.stringify({
        title: title.trim(),
        description: description.trim(),
        join,
        parts: linesOf(parts),
        then: thenId,
      });
    }
    return JSON.stringify({
      title: title.trim(),
      description: description.trim(),
      outcomeId,
      enabled: [...enabled].sort(),
    });
  }

  function fill() {
    title = open?.title || "";
    description = open?.description || "";
    const claim = open?.formula ? readClaim(open.formula) : null;
    const bare = open?.formula?.trim().match(/^fact:([A-Za-z0-9_]+)$/);
    legacy = Boolean(open && claim && !bare);
    join = claim?.join || "and";
    parts = claim ? [...claim.partIds] : ["", ""];
    thenId = claim?.thenId || "";
    outcomeId = bare?.[1] || "";
    enabled = [...(open?.assumes || [])];
    base = current();
  }

  $effect(() => {
    const key = drafting ? "new:assert" : open ? `assert:${open.id}` : "";
    if (key === seen) return;
    seen = key;
    editing = false;
    logs = false;
    fill();
  });

  function cancel() {
    if (drafting) oncancel();
    else {
      fill();
      editing = false;
    }
  }

  async function submit(event) {
    event.preventDefault();
    if (!dirty) return;
    let formula = "";
    let assumes = [];
    if (legacy) {
      const picked = linesOf(parts);
      const known = new Set(options.map((item) => item.id));
      if (!picked.length || !thenId || picked.some((id) => !known.has(id)) || !known.has(thenId)) {
        onerror(new Error("pick a fact, criterion, or theorem that exists"));
        return;
      }
      formula = compileClaim(join, picked, thenId);
    } else {
      if (!outcomes.some((item) => item.id === outcomeId)) {
        onerror(new Error("pick an outcome"));
        return;
      }
      formula = `fact:${outcomeId}`;
      assumes = enabled.filter((id) => criteria.some((item) => item.id === id));
    }
    const taken = new Set([
      ...facts.map((fact) => fact.id),
      ...asserts.map((item) => item.id),
      ...Object.keys(labels),
    ]);
    if (open) taken.delete(open.id);
    const body = {
      title: title.trim(),
      description: description.trim(),
      formula,
      assumes,
      labels: {},
    };
    try {
      if (open) await onsave(body, open.id);
      else await onsave({ id: uniqueSlug(body.title, taken), ...body });
      editing = false;
      base = current();
    } catch (err) {
      onerror(err);
    }
  }

  async function toggleSearch() {
    if (searching) {
      searching = false;
      query = "";
      return;
    }
    searching = true;
    await tick();
    finder?.focus();
  }

  async function remove() {
    if (!confirm("delete this assert?")) return;
    try {
      await onremove();
    } catch (err) {
      onerror(err);
    }
  }
</script>

<section class="shell">
  <aside class="rail">
    <div class="spread">
      <h2>asserts</h2>
      <span class="actions">
        <button
          class="icon"
          type="button"
          aria-label={searching ? "close search" : "search"}
          title={searching ? "close search" : "search"}
          onclick={toggleSearch}
        >
          <Icon name={searching ? "x" : "search"} />
        </button>
        {#if canWrite}
          <button class="icon" type="button" aria-label="add" title="add" onclick={oncreate}>
            <Icon name="plus" />
          </button>
        {/if}
      </span>
    </div>
    {#if searching}
      <input class="finder" placeholder="search" bind:value={query} bind:this={finder} />
    {/if}
    <div class="list">
      {#each visible as assert (assert.id)}
        {@const state = assertState(assert)}
        <button
          class="pick assert-pick"
          class:good={state === "proved"}
          class:selected={open?.id === assert.id}
          type="button"
          title={blurb(assert.formula)}
          onclick={() => onchoose(assert.id)}
        >
          <span class="seal-dot {state}"></span>
          <span>{assertTitle(assert)}</span>
        </button>
      {/each}
    </div>
  </aside>

  {#if open || forming}
  <div class="detail">
    {#if open && !forming}
    <div class="stage">
      <div class="spread">
        <p class="kicker">assert</p>
        <div class="actions">
          <span class="seal {status}">
            {#if status === "proved"}proved
            {:else if status === "invalid"}rejected
            {:else}checking{/if}
          </span>
          {#if open && status === "proved"}
            <button class="quiet" type="button" onclick={() => onexport(open)}>export</button>
          {/if}
          {#if canWrite}
            <button class="icon" type="button" aria-label="edit" title="edit" onclick={() => (editing = true)}>
              <Icon name="pencil" />
            </button>
          {/if}
        </div>
      </div>
      <div class="spread head-line">
        <h2 class="item-title">{fillName(open.title, personName || "someone")}</h2>
        {#if people.length}
          <div class="who" role="group" aria-label="whose facts to use">
            <span>for</span>
            <button type="button" class:on={!personId} onclick={() => onperson("")}>the library</button>
            {#each people as item (item.id)}
              <button type="button" class:on={personId === item.id} onclick={() => onperson(item.id)}>{item.name}</button>
            {/each}
          </div>
        {/if}
      </div>
      {#if open.description}
        <p class="dek">{open.description}</p>
      {/if}
      {#if open.assumes?.length}
        <p class="dek">Assuming {open.assumes.map((id) => nameOf(id)).join(", ")}.</p>
      {/if}
      {#if shown}
        <ClaimView join={shown.join} parts={shown.partIds.map(phrase)} thenTitle={phrase(shown.thenId)} onopen={onopenFact} />
      {/if}
      {#if status === "proved" && tree}
        <div class="proof">
          {#key `${open.id}:${personId}`}
            <ProofDag {tree} onopen={onopenFact} />
          {/key}
        </div>
      {:else if status === "invalid"}
        <p class="dek reject">
          This assertion does not prove.
          {#if record?.diagnostics}
            <button
              class="icon"
              type="button"
              aria-label={logs ? "hide logs" : "show logs"}
              title={logs ? "hide logs" : "show logs"}
              onclick={() => (logs = !logs)}
            >
              <Icon name="eye" />
            </button>
          {/if}
        </p>
        {#if logs && record?.diagnostics}
          <pre class="diagnostic">{record.diagnostics}</pre>
        {/if}
      {/if}
    </div>
      {#if personView?.missing?.length}
        <p class="missing">To make this hold, turn on: {personView.missing.map((item) => item.claim).join(", ")}.</p>
      {/if}

      {#if open}
        <Comments
          {thread}
          {username}
          {canWrite}
          onadd={(body) => oncomment("assert", open.id, body)}
          onedit={oneditcomment}
          onremove={ondeletecomment}
          {onresolve}
        />
      {/if}
    {:else if forming}
      <form class="stack" onsubmit={submit}>
        <div class="spread">
          <p class="kicker">{open ? "assert" : "new assert"}</p>
          {#if open}
            <button class="icon danger" type="button" aria-label="delete" title="delete" onclick={remove}>
              <Icon name="trash" />
            </button>
          {/if}
        </div>
        <label>title <input bind:value={title} /></label>
        <label>description <textarea bind:value={description}></textarea></label>
        {#if legacy}
          <ClaimFields bind:join bind:parts bind:thenId {options} />
        {:else}
          <label>
            outcome
            <select bind:value={outcomeId}>
              <option value="">pick an outcome</option>
              {#each outcomes as item (item.id)}
                <option value={item.id}>{item.title}</option>
              {/each}
            </select>
          </label>
          <p class="kicker">criteria to assume</p>
          <ul class="toggles">
            {#each criteria as item (item.id)}
              <li>
                <label class="fact-toggle">
                  <input
                    type="checkbox"
                    checked={enabled.includes(item.id)}
                    onchange={(event) => {
                      enabled = event.currentTarget.checked
                        ? [...enabled, item.id]
                        : enabled.filter((id) => id !== item.id);
                    }}
                  />
                  <span>{item.title}</span>
                </label>
              </li>
            {/each}
          </ul>
        {/if}
        <div class="actions">
          {#if dirty}
            <button class="icon primary" type="submit" aria-label="save" title="save">
              <Icon name="check" />
            </button>
          {/if}
          <button class="icon quiet" type="button" aria-label="cancel" title="cancel" onclick={cancel}>
            <Icon name="x" />
          </button>
        </div>
      </form>
    {/if}
  </div>
  {/if}
</section>
