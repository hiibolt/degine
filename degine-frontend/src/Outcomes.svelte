<script>
  import { compileClaim, readClaim, uniqueSlug } from "./phrases.js";
  import { outcomeBoard } from "./outcomes.js";
  import Icon from "./Icon.svelte";
  import ProofDag from "./ProofDag.svelte";
  import ClaimView from "./ClaimView.svelte";

  let { facts = [], labels = {}, asserts = [], owner = false, onimport, onopenFact } = $props();

  let picked = $state("");
  let openKey = $state("");

  function nameOf(id) {
    return facts.find((fact) => fact.id === id && !fact.formula)?.claim || labels[id] || id;
  }

  const board = $derived(outcomeBoard(facts, labels, nameOf));
  const ordered = $derived(
    [...board].sort((a, b) => Number(needed(a.id)) - Number(needed(b.id))),
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
      <p class="kicker">outcome</p>
      <h2 class="item-title">{current.title}</h2>
      {#each current.ways as way (way.key)}
        <article class="combo">
          <div class="spread">
            <ClaimView join="and" parts={way.shown.map(phrase)} thenTitle={{ title: current.title }} onopen={onopenFact} />
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
