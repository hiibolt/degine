<script>
  import { compileClaim, fillName, readClaim, uniqueSlug } from "./phrases.js";
  import { outcomeBoard } from "./outcomes.js";
  import Icon from "./Icon.svelte";
  import ProofDag from "./ProofDag.svelte";
  import ClaimView from "./ClaimView.svelte";

  let {
    facts = [],
    labels = {},
    asserts = [],
    personal = [],
    people = [],
    personId = "",
    onperson = () => {},
    owner = false,
    onimport,
    onopenFact,
  } = $props();

  let picked = $state("");
  let openKey = $state("");

  const person = $derived(people.find((item) => item.id === personId) || null);

  const who = $derived(person?.name || "someone");

  const scoped = $derived.by(() => {
    const on = new Set(person?.on || []);
    const extra = personal
      .filter((item) => !person || on.has(item.id))
      .map((item) => ({
        id: item.id,
        claim: fillName(item.claim, who),
        role: person ? "fact" : "criterion",
        formula: null,
      }));
    return [...facts, ...extra];
  });

  function nameOf(id) {
    const raw = scoped.find((fact) => fact.id === id && !fact.formula)?.claim || labels[id] || id;
    return fillName(raw, who);
  }

  const board = $derived(outcomeBoard(scoped, labels, nameOf));
  const ordered = $derived(
    [...board].sort(
      (a, b) => Number(needed(a.id)) - Number(needed(b.id)) || (a.sort || a.id).localeCompare(b.sort || b.id),
    ),
  );
  const current = $derived(ordered.find((item) => item.id === picked) || ordered[0] || null);

  function needed(id) {
    const claims = [
      ...facts.map((fact) => (fact.formula ? readClaim(fact.formula) : null)),
      ...asserts.map((item) => readClaim(item.formula || "")),
    ];
    return claims.some((claim) => claim?.partIds.includes(id));
  }

  function phrase(id) {
    const fact = facts.find((item) => item.id === id);
    return { id: fact ? id : "", title: nameOf(id) };
  }

  async function bring(outcome, way) {
    const parts = way.shown;
    if (!parts.length) return;
    const names = parts.map(nameOf);
    const title =
      way.assumes.length > 1
        ? `If ${names.join(" and ")}, ${outcome.title}`
        : way.assumes.length === 1
          ? `If ${names[0]}, ${outcome.title}`
          : outcome.title;
    const taken = new Set([
      ...facts.map((fact) => fact.id),
      ...asserts.map((item) => item.id),
      ...Object.keys(labels),
    ]);
    await onimport({
      id: uniqueSlug(title, taken),
      title,
      description: "",
      formula: compileClaim("and", parts, outcome.id),
      labels: {},
    });
  }
</script>

<div class="shell">
  <aside class="rail">
    <h2>outcomes</h2>
    <div class="list">
      {#each ordered as item (item.id)}
        <button
          class="pick"
          class:good={item.ways.some((way) => !way.assumes.length)}
          class:bad={!item.ways.length}
          class:open={item.ways.length > 0 && item.ways.every((way) => way.assumes.length)}
          class:selected={current?.id === item.id}
          type="button"
          onclick={() => {
            picked = item.id;
            openKey = "";
          }}
        >
          {item.title}
        </button>
      {/each}
    </div>
  </aside>

  {#if current}
    <section class="detail">
      <div class="spread head-line">
        <p class="kicker">outcome</p>
        {#if people.length}
          <div class="who" role="group" aria-label="whose facts to use">
            <span>using</span>
            <button type="button" class:on={!personId} onclick={() => onperson("")}>someone</button>
            {#each people as item (item.id)}
              <button type="button" class:on={personId === item.id} onclick={() => onperson(item.id)}>{item.name}</button>
            {/each}
          </div>
        {/if}
      </div>
      <h2 class="item-title">{current.title}</h2>
      {#if !current.ways.length}
        <p class="dek">Nothing reaches this.</p>
      {/if}
      {#each current.ways as way (way.key)}
        <article class="combo">
          <div class="spread">
            {#if way.assumes.length}
              <ClaimView join="and" parts={way.shown.map(phrase)} thenTitle={{ title: current.title }} onopen={onopenFact} />
            {:else}
              <span class="seal proved">proved</span>
            {/if}
            <div class="actions">
              <button
                class="icon"
                type="button"
                aria-label="proof"
                title="proof"
                onclick={() => (openKey = openKey === way.key ? "" : way.key)}
              >
                <Icon name="eye" />
              </button>
              {#if owner}
                <button
                  class="icon"
                  type="button"
                  aria-label="import as assert"
                  title="import as assert"
                  onclick={() => bring(current, way)}
                >
                  <Icon name="plus" />
                </button>
              {/if}
            </div>
          </div>
          {#if openKey === way.key}
            <div class="proof">
              <ProofDag tree={way.tree} onopen={onopenFact} unfold />
            </div>
          {/if}
        </article>
      {/each}
    </section>
  {/if}
</div>
