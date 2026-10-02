<script>
  import { fillName } from "./phrases.js";

  let { people = [], personal = [], owner = false, onaddPerson, onremovePerson, onaddFact, ontoggle, onerror } =
    $props();

  let personId = $state("");
  let name = $state("");
  let factId = $state("");
  let claim = $state("");

  const person = $derived(people.find((item) => item.id === personId) || people[0] || null);
  const listedPeople = $derived([...people].sort((a, b) => a.name.localeCompare(b.name)));
  const listedFacts = $derived([...personal].sort((a, b) => a.claim.localeCompare(b.claim)));
  const on = $derived(new Set(person?.on || []));

  function slug(value) {
    return value.trim().toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");
  }

  async function addPerson(event) {
    event.preventDefault();
    const id = slug(name);
    if (!id) return;
    try {
      await onaddPerson({ id, name: name.trim() });
      personId = id;
      name = "";
    } catch (err) {
      onerror(err);
    }
  }

  async function addFact(event) {
    event.preventDefault();
    const id = slug(factId || claim);
    if (!id || !claim.trim()) return;
    try {
      await onaddFact({ id, claim: claim.trim() });
      factId = "";
      claim = "";
    } catch (err) {
      onerror(err);
    }
  }
</script>

<div class="shell">
  <aside class="rail">
    <p class="kicker">people</p>
    <div class="list">
      {#each listedPeople as item (item.id)}
        <button class="pick" class:selected={person?.id === item.id} type="button" onclick={() => (personId = item.id)}>
          {item.name}
        </button>
      {/each}
    </div>
    {#if owner}
      <form class="stack add-person" onsubmit={addPerson}>
        <label>name <input bind:value={name} placeholder="name" /></label>
        <button class="primary" type="submit">add person</button>
      </form>
    {/if}
  </aside>
  <div class="detail">
    {#if person}
      <div class="spread">
        <h2 class="item-title">{person.name}</h2>
        {#if owner}
          <button class="quiet" type="button" onclick={() => onremovePerson(person.id)}>remove</button>
        {/if}
      </div>
      <ul class="toggles">
        {#each listedFacts as fact (fact.id)}
          <li>
            <label class="fact-toggle">
              <input
                type="checkbox"
                checked={on.has(fact.id)}
                disabled={!owner}
                onchange={(event) => ontoggle(person.id, fact.id, event.currentTarget.checked)}
              />
              <span>{fillName(fact.claim, person.name)}</span>
            </label>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="dek">No people yet.</p>
    {/if}
    {#if owner}
      <form class="stack add-fact" onsubmit={addFact}>
        <p class="kicker">new personal fact</p>
        <label>wording, use {'{name}'} <input bind:value={claim} placeholder="{'{name}'} does X" /></label>
        <button class="primary" type="submit">add fact</button>
      </form>
    {/if}
  </div>
</div>
