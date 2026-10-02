<script>
  import { tick } from "svelte";
  import ClaimFields from "./ClaimFields.svelte";
  import ClaimView from "./ClaimView.svelte";
  import Comments from "./Comments.svelte";
  import Icon from "./Icon.svelte";
  import ProofDag from "./ProofDag.svelte";
  import ShareMenu from "./ShareMenu.svelte";
  import { atomChoices, compileClaim, readClaim, uniqueSlug } from "./phrases.js";
  import { explainAssert } from "./proof.js";
  import { shortDiagnostic } from "./prose.js";

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
    owner,
    username,
    shares,
    comments,
    oncomment,
    oneditcomment,
    ondeletecomment,
    onresolve,
    onexport,
    onshares,
    ontoggleShare,
  } = $props();

  let title = $state("");
  let description = $state("");
  let join = $state("and");
  let parts = $state(["", ""]);
  let thenId = $state("");
  let editing = $state(false);
  let base = $state("");
  let query = $state("");
  let searching = $state(false);
  let finder = $state(null);
  let seen = "";

  function blurb(formula) {
    const claim = readClaim(formula);
    if (!claim) return formula;
    const left = claim.partIds.map(nameOf).join(claim.join === "or" ? " or " : " and ");
    return `if ${left}, then ${nameOf(claim.thenId)}`;
  }

  function nameOf(id) {
    return facts.find((fact) => fact.id === id && !fact.formula)?.claim || labels[id] || id;
  }

  function linesOf(items) {
    return items.map((part) => part.trim()).filter(Boolean);
  }

  const open = $derived(
    !drafting && selected?.kind === "assert" ? asserts.find((item) => item.id === selected.id) : null,
  );
  const record = $derived(open ? graphs[open.id] : null);
  const status = $derived(record?.status || (open ? "pending" : ""));
  const titleOf = (id) => nameOf(id);
  const tree = $derived(
    open ? explainAssert(open.formula, facts, titleOf, record?.status === "proved" ? record.graph : null) : null,
  );
  const shown = $derived(open?.formula ? readClaim(open.formula) : null);
  const options = $derived(atomChoices(facts, labels));
  const visible = $derived(
    asserts.filter((item) => {
      const needle = query.trim().toLowerCase();
      if (!needle) return true;
      return item.title.toLowerCase().includes(needle) || blurb(item.formula).toLowerCase().includes(needle);
    }),
  );
  const mine = $derived(visible.filter((item) => !item.shared));
  const sharedAsserts = $derived(visible.filter((item) => item.shared));

  $effect(() => {
    if (drafting || open) return;
    const first = mine[0] || sharedAsserts[0];
    if (first) onchoose(first.id);
  });
  const thread = $derived(open ? comments[`assert:${open.id}`] || [] : []);
  const forming = $derived(Boolean(drafting) || editing);
  const dirty = $derived(current() !== base);

  function phrase(id) {
    const fact = facts.find((item) => item.id === id && item.role !== "theorem" && !item.formula);
    return { id: fact ? id : "", title: nameOf(id) };
  }

  function current() {
    return JSON.stringify({
      title: title.trim(),
      description: description.trim(),
      join,
      parts: linesOf(parts),
      then: thenId,
    });
  }

  function fill() {
    title = open?.title || "";
    description = open?.description || "";
    const claim = open?.formula ? readClaim(open.formula) : null;
    join = claim?.join || "and";
    parts = claim ? [...claim.partIds] : ["", ""];
    thenId = claim?.thenId || "";
    base = current();
  }

  $effect(() => {
    const key = drafting ? "new:assert" : open ? `assert:${open.id}` : "";
    if (key === seen) return;
    seen = key;
    editing = false;
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
    const picked = linesOf(parts);
    const known = new Set(options.map((item) => item.id));
    if (!picked.length || !thenId || picked.some((id) => !known.has(id)) || !known.has(thenId)) {
      onerror(new Error("pick a fact, criterion, or theorem that exists"));
      return;
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
      formula: compileClaim(join, picked, thenId),
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
        {#if owner}
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
      {#each mine as assert (assert.id)}
        {@const state = graphs[assert.id]?.status || "pending"}
        <button
          class="pick assert-pick"
          class:selected={open?.id === assert.id}
          type="button"
          title={blurb(assert.formula)}
          onclick={() => onchoose(assert.id)}
        >
          <span class="seal-dot {state}"></span>
          <span>{assert.title}</span>
        </button>
      {/each}
      {#if sharedAsserts.length}
        <h3>shared</h3>
        {#each sharedAsserts as assert (assert.id)}
          {@const state = graphs[assert.id]?.status || "pending"}
          <button
            class="pick assert-pick"
            class:selected={open?.id === assert.id}
            type="button"
            title={blurb(assert.formula)}
            onclick={() => onchoose(assert.id)}
          >
            <span class="seal-dot {state}"></span>
            <span>{assert.title}</span>
          </button>
        {/each}
      {/if}
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
          {#if open}
            <ShareMenu
              {owner}
              {shares}
              onexport={() => onexport(open)}
              onload={() => onshares(open.id)}
              ontoggle={(person, on) => ontoggleShare(open.id, person, on)}
            />
          {/if}
          {#if owner}
            <button class="icon" type="button" aria-label="edit" title="edit" onclick={() => (editing = true)}>
              <Icon name="pencil" />
            </button>
          {/if}
        </div>
      </div>
      <h2 class="item-title">{open.title}</h2>
      {#if open.description}
        <p class="dek">{open.description}</p>
      {/if}
      {#if shown}
        <ClaimView join={shown.join} parts={shown.partIds.map(phrase)} thenTitle={phrase(shown.thenId)} onopen={onopenFact} />
      {/if}
      {#if tree}
        <div class="proof">
          {#key open.id}
            <ProofDag {tree} onopen={onopenFact} />
          {/key}
        </div>
      {/if}
    </div>
      {#if record?.diagnostics}
        <pre class="diagnostic">{shortDiagnostic(record.diagnostics)}</pre>
      {/if}
      {#if open}
        <Comments
          {thread}
          {username}
          {owner}
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
        <ClaimFields bind:join bind:parts bind:thenId {options} />
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
