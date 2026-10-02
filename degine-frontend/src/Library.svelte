<script>
  import { tick } from "svelte";
  import ClaimFields from "./ClaimFields.svelte";
  import ClaimView from "./ClaimView.svelte";
  import Comments from "./Comments.svelte";
  import Icon from "./Icon.svelte";
  import ProofDag from "./ProofDag.svelte";
  import { atomChoices, compileClaim, fillName, readClaim, uniqueSlug } from "./phrases.js";
  import { explainItem } from "./proof.js";

  let {
    facts,
    labels,
    asserts,
    selected,
    drafting,
    onsave,
    onremove,
    onchoose,
    ondraft,
    oncancel,
    onerror,
    onopenAssert,
    owner,
    username,
    comments,
    oncomment,
    oneditcomment,
    ondeletecomment,
    onresolve,
    onderive,
  } = $props();

  let title = $state("");
  let citations = $state("");
  let join = $state("and");
  let parts = $state([""]);
  let thenId = $state("");
  let thenName = $state("");
  let editing = $state(false);
  let base = $state("");
  let theoremQuery = $state("");
  let searching = $state(false);
  let finder = $state(null);
  let seen = "";
  let deriving = $state(false);
  let lowerTitle = $state("");

  const kind = $derived(drafting || (selected?.kind === "fact" ? openKind(selected.id) : ""));
  const open = $derived(
    !drafting && selected?.kind === "fact" ? facts.find((fact) => fact.id === selected.id) : null,
  );
  const listed = $derived({
    fact: facts.filter((fact) => roleOf(fact) === "fact" && !fact.shared).sort(byTitle),
    criterion: facts.filter((fact) => roleOf(fact) === "criterion" && !fact.shared).sort(byTitle),
    theorem: facts.filter((fact) => roleOf(fact) === "theorem" && !fact.shared).sort(byTitle),
  });
  const sharedFacts = $derived(facts.filter((fact) => fact.shared).sort(byTitle));
  const thread = $derived(open ? comments[`fact:${open.id}`] || [] : []);
  const theorems = $derived(
    listed.theorem.filter((fact) => fact.claim.toLowerCase().includes(theoremQuery.trim().toLowerCase())),
  );

  $effect(() => {
    if (drafting || open) return;
    const first = listed.fact[0] || listed.criterion[0] || listed.theorem[0] || sharedFacts[0];
    if (first) onchoose(first.id);
  });
  const forming = $derived(Boolean(drafting) || editing || deriving);
  const dirty = $derived(current() !== base);
  const shown = $derived(open?.formula ? readClaim(open.formula) : null);
  const options = $derived(atomChoices(facts, labels));
  const itemTree = $derived(open ? explainItem(open, facts, nameOf) : null);
  const users = $derived(open ? usedBy(open) : []);

  const notes = {
    fact: "Cited things taken as true.",
    criterion: "A condition a theorem can ask about. Not claimed, so it needs no citation.",
    theorem: "An if-then. Any-of can hold one case for now, and grow later.",
  };

  function byTitle(a, b) {
    return a.claim.localeCompare(b.claim);
  }

  function roleOf(fact) {
    if (fact.role === "fact" || fact.role === "criterion" || fact.role === "theorem") return fact.role;
    return fact.formula ? "theorem" : "fact";
  }

  const toneOf = $derived.by(() => {
    const byId = new Map(facts.map((fact) => [fact.id, fact]));
    const byThen = new Map();
    for (const fact of facts) {
      const claim = fact.formula ? readClaim(fact.formula) : null;
      if (!claim) continue;
      const list = byThen.get(claim.thenId) || [];
      list.push(claim);
      byThen.set(claim.thenId, list);
    }
    const memo = new Map();
    function atom(id, stack) {
      if (memo.has(id)) return memo.get(id);
      const fact = byId.get(id);
      if (fact && !fact.formula) return fact.role === "criterion" ? "open" : "good";
      if (stack.has(id)) return "open";
      const next = new Set(stack);
      next.add(id);
      const claims = byThen.get(id) || [];
      if (!claims.length) return "open";
      const tones = claims.map((claim) => claimTone(claim, next));
      const tone = tones.every((item) => item === "good") ? "good" : "open";
      memo.set(id, tone);
      return tone;
    }
    function claimTone(claim, stack) {
      const tones = claim.partIds.map((id) => atom(id, stack));
      if (claim.join === "or") return tones.some((item) => item === "good") ? "good" : "open";
      return tones.every((item) => item === "good") ? "good" : "open";
    }
    const tones = new Map();
    for (const fact of facts) {
      if (!fact.formula) {
        tones.set(fact.id, fact.role === "criterion" ? "open" : "good");
        continue;
      }
      const claim = readClaim(fact.formula);
      tones.set(fact.id, claim ? claimTone(claim, new Set()) : "open");
    }
    return tones;
  });

  function tone(fact) {
    return toneOf.get(fact.id) || "";
  }

  function openKind(id) {
    const fact = facts.find((item) => item.id === id);
    return fact ? roleOf(fact) : "";
  }

  function someone(text) {
    return fillName(text);
  }

  function nameOf(id) {
    return someone(facts.find((fact) => fact.id === id && !fact.formula)?.claim || labels[id] || id);
  }

  function lines(value) {
    return value
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  }

  function usedBy(item) {
    const role = roleOf(item);
    const atom = role === "theorem" ? readClaim(item.formula || "")?.thenId : item.id;
    if (!atom) return [];
    const rows = [];
    for (const fact of facts) {
      if (roleOf(fact) !== "theorem" || !fact.formula || fact.id === item.id) continue;
      const claim = readClaim(fact.formula);
      if (!claim) continue;
      if (claim.partIds.includes(atom) || claim.thenId === atom) {
        rows.push({ id: fact.id, title: someone(fact.claim), kind: "fact" });
      }
    }
    for (const assert of asserts) {
      const claim = readClaim(assert.formula || "");
      if (!claim) continue;
      if (claim.partIds.includes(atom) || claim.thenId === atom) {
        rows.push({ id: assert.id, title: assert.title, kind: "assert" });
      }
    }
    return rows;
  }

  function current() {
    return JSON.stringify({
      title: title.trim(),
      citations: kind === "criterion" ? [] : lines(citations),
      join,
      parts: parts.map((part) => part.trim()).filter(Boolean),
      then: thenId,
      thenName: thenName.trim(),
    });
  }

  function fill() {
    title = open?.claim || "";
    citations = open?.citations.join("\n") || "";
    const claim = open?.formula ? readClaim(open.formula) : null;
    join = claim?.join || (drafting === "theorem" ? "or" : "and");
    parts = claim ? [...claim.partIds] : [""];
    thenId = claim?.thenId || "";
    thenName = thenId ? nameOf(thenId) : "";
    base = current();
  }

  $effect(() => {
    const key = drafting ? `new:${drafting}` : open ? `fact:${open.id}` : "";
    if (key === seen) return;
    seen = key;
    editing = false;
    fill();
  });

  function takenIds() {
    return new Set([
      ...facts.map((fact) => fact.id),
      ...asserts.map((item) => item.id),
      ...Object.keys(labels),
    ]);
  }

  function settleOutcome(taken) {
    const named = thenName.trim();
    const known = options.find((item) => item.id === thenId && item.role !== "theorem");
    const hit = options.find(
      (item) =>
        item.role !== "theorem" &&
        item.id !== known?.id &&
        item.title.toLowerCase() === named.toLowerCase(),
    );
    if (hit) return { id: hit.id, labels: {} };
    if (known?.role === "outcome") {
      if (known.title !== named) return { id: known.id, labels: { [known.id]: named } };
      return { id: known.id, labels: {} };
    }
    if (known && known.title === named) return { id: known.id, labels: {} };
    const id = uniqueSlug(named, taken);
    return { id, labels: { [id]: named } };
  }

  async function submit(event) {
    event.preventDefault();
    if (!dirty) return;
    const making = drafting || (open ? roleOf(open) : "fact");
    const taken = takenIds();
    let formula = null;
    let labelsOut = {};
    if (making === "theorem") {
      const picked = parts.map((part) => part.trim()).filter(Boolean);
      const known = new Set(options.map((item) => item.id));
      if (!picked.length || picked.some((id) => !known.has(id))) {
        onerror(new Error("pick a fact, criterion, or theorem that exists"));
        return;
      }
      if (!thenName.trim()) {
        onerror(new Error("name the result"));
        return;
      }
      const outcome = settleOutcome(taken);
      if (!outcome.id) {
        onerror(new Error("name the result"));
        return;
      }
      taken.add(outcome.id);
      labelsOut = outcome.labels;
      formula = compileClaim(join, picked, outcome.id);
    }
    if (open) taken.delete(open.id);
    const body = {
      claim: title.trim(),
      citations: making === "criterion" ? [] : lines(citations),
      formula,
      role: making,
    };
    if (making !== "criterion" && !body.citations.length && !confirm("save this with no citation?")) return;
    try {
      const id = open ? open.id : uniqueSlug(body.claim, taken);
      if (!id) {
        onerror(new Error("give it a title with a letter or a number"));
        return;
      }
      await onsave({ id, ...body, labels: labelsOut }, open?.id);
      editing = false;
      base = current();
    } catch (err) {
      onerror(err);
    }
  }

  function cancel() {
    if (drafting) oncancel();
    else {
      fill();
      editing = false;
    }
  }

  async function remove() {
    if (!confirm("delete this?")) return;
    try {
      await onremove();
    } catch (err) {
      onerror(err);
    }
  }

  function phrase(id) {
    const fact = facts.find((item) => item.id === id && roleOf(item) !== "theorem");
    return { id: fact ? id : "", title: nameOf(id) };
  }

  function openUser(item) {
    if (item.kind === "assert") onopenAssert(item.id);
    else onchoose(item.id);
  }

  async function toggleSearch() {
    if (searching) {
      searching = false;
      theoremQuery = "";
      return;
    }
    searching = true;
    await tick();
    finder?.focus();
  }
</script>

<div class="shell">
  <aside class="rail">
    {#if owner}
    <div class="group">
      <div class="spread">
        <h2 title={notes.fact}>facts</h2>
        <button class="icon" type="button" aria-label="add" title="add" onclick={() => ondraft("fact")}>
          <Icon name="plus" />
        </button>
      </div>
      <div class="list">
        {#each listed.fact as fact (fact.id)}
          <button class="pick" class:selected={open?.id === fact.id} type="button" onclick={() => onchoose(fact.id)}>
            <span class="seal-dot {tone(fact) === 'good' ? 'proved' : 'open'}"></span>
            <span>{someone(fact.claim)}</span>
          </button>
        {/each}
      </div>
    </div>
    <div class="group">
      <div class="spread">
        <h2 title={notes.criterion}>criteria</h2>
        <button class="icon" type="button" aria-label="add" title="add" onclick={() => ondraft("criterion")}>
          <Icon name="plus" />
        </button>
      </div>
      <div class="list">
        {#each listed.criterion as fact (fact.id)}
          <button class="pick" class:selected={open?.id === fact.id} type="button" onclick={() => onchoose(fact.id)}>
            <span class="seal-dot {tone(fact) === 'good' ? 'proved' : 'open'}"></span>
            <span>{someone(fact.claim)}</span>
          </button>
        {/each}
      </div>
    </div>
    <div class="group">
      <div class="spread">
        <h2 title={notes.theorem}>theorems</h2>
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
          <button class="icon" type="button" aria-label="add" title="add" onclick={() => ondraft("theorem")}>
            <Icon name="plus" />
          </button>
        </span>
      </div>
      {#if searching}
        <input class="finder" placeholder="search" bind:value={theoremQuery} bind:this={finder} />
      {/if}
      <div class="list">
        {#each theorems as fact (fact.id)}
          <button class="pick" class:selected={open?.id === fact.id} type="button" onclick={() => onchoose(fact.id)}>
            <span class="seal-dot {tone(fact) === 'good' ? 'proved' : 'open'}"></span>
            <span>{someone(fact.claim)}</span>
          </button>
        {/each}
      </div>
    </div>
    {/if}
    {#if sharedFacts.length}
      <details class="group">
        <summary>shared</summary>
        <div class="list">
          {#each sharedFacts as fact (fact.id)}
            <button class="pick" class:selected={open?.id === fact.id} type="button" onclick={() => onchoose(fact.id)}>
              <span class="seal-dot {tone(fact) === 'good' ? 'proved' : 'open'}"></span>
              <span>{someone(fact.claim)}</span>
            </button>
          {/each}
        </div>
      </details>
    {/if}
  </aside>

  {#if kind}
  <section class="detail">
      {#if !forming}
      <div class="stage">
        <div class="spread">
          <p class="kicker">{kind}</p>
          {#if owner}
            {#if kind === "fact"}
              <button class="quiet" type="button" onclick={() => (deriving = true)}>make a theorem</button>
            {/if}
            <button class="icon" type="button" aria-label="edit" title="edit" onclick={() => (editing = true)}>
              <Icon name="pencil" />
            </button>
          {/if}
        </div>
        <h2 class="item-title">{someone(open.claim)}</h2>
        {#if shown}
          <ClaimView
            join={shown.join}
            parts={shown.partIds.map(phrase)}
            thenTitle={phrase(shown.thenId)}
            onopen={onchoose}
          />
        {/if}
        {#if open.citations.length}
          <ul class="cites">
            {#each open.citations as cite}
              <li>
                {#if cite.startsWith("http://") || cite.startsWith("https://")}
                  <a href={cite} target="_blank" rel="noreferrer">{cite}</a>
                {:else}
                  {cite}
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if itemTree}
          <div class="proof">
            {#key open.id}
              <ProofDag tree={itemTree} onopen={onchoose} />
            {/key}
          </div>
        {/if}
      </div>
        <section class="uses">
          <h3>{kind === "criterion" ? "criteria for" : "depended on by"}</h3>
          <ul>
            {#each users as item (`${item.kind}:${item.id}`)}
              <li>
                <span>{item.title}</span>
                <button class="icon" type="button" aria-label="view" title="view" onclick={() => openUser(item)}>
                  <Icon name="eye" />
                </button>
              </li>
            {/each}
          </ul>
        </section>
        {#if open}
          <Comments
            {thread}
            {username}
            {owner}
            onadd={(body) => oncomment("fact", open.id, body)}
            onedit={oneditcomment}
            onremove={ondeletecomment}
            {onresolve}
          />
        {/if}
      {:else if deriving && open}
        <form class="stack" onsubmit={async (event) => {
          event.preventDefault();
          const claim = lowerTitle.trim();
          if (!claim) return onerror(new Error("name the new fact"));
          const taken = new Set([...facts.map((fact) => fact.id), ...Object.keys(labels || {})]);
          const id = uniqueSlug(claim, taken);
          if (!id || id === open.id) return onerror(new Error("give the new fact its own name"));
          try {
            await onderive(open.id, id, claim);
            deriving = false;
            lowerTitle = "";
          } catch (err) {
            onerror(err);
          }
        }}>
          <p class="kicker">make a theorem</p>
          <p>the statement and its citations become a theorem. it depends on this new fact. the old id stays the conclusion.</p>
          <label>
            new fact
            <input bind:value={lowerTitle} />
          </label>
          <button class="primary" type="submit">convert</button>
          <button class="quiet" type="button" onclick={() => (deriving = false)}>cancel</button>
        </form>
      {:else}
        <form class="stack" onsubmit={submit}>
          <div class="spread">
            <p class="kicker">{open ? kind : `new ${kind}`}</p>
            {#if open}
              <button class="icon danger" type="button" aria-label="delete" title="delete" onclick={remove}>
                <Icon name="trash" />
              </button>
            {/if}
          </div>
          <label>
            title
            <input bind:value={title} />
          </label>
          {#if kind !== "criterion"}
            <label>
              citations
              <textarea bind:value={citations}></textarea>
            </label>
          {/if}
          {#if kind === "theorem"}
            <ClaimFields bind:join bind:parts bind:thenId bind:thenName mintThen {options} />
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
  </section>
  {/if}
</div>
