<script>
  let { people = [], personal = [], owner = false, onaddPerson, onremovePerson, onaddFact, ontoggle, onerror } =
    $props();

  let personId = $state("");
  let name = $state("");
  let factId = $state("");
  let claim = $state("");

  const person = $derived(people.find((item) => item.id === personId) || people[0] || null);
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

<div class="library">
  <aside class="rail">
    <p class="kicker">people</p>
    {#each people as item (item.id)}
      <button class="row" class:selected={person?.id === item.id} type="button" onclick={() => (personId = item.id)}>
        <span>{item.name}</span>
      </button>
    {/each}
    {#if owner}
      <form class="stack" onsubmit={addPerson}>
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
      <p class="dek">Each line is one fact. On means it holds for {person.name}.</p>
      <ul class="toggles">
        {#each personal as fact (fact.id)}
          <li>
            <label>
              <input
                type="checkbox"
                checked={on.has(fact.id)}
                disabled={!owner}
                onchange={(event) => ontoggle(person.id, fact.id, event.currentTarget.checked)}
              />
              {fact.claim.replaceAll("{name}", person.name)}
            </label>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="dek">No people yet.</p>
    {/if}
    {#if owner}
      <form class="stack" onsubmit={addFact}>
        <p class="kicker">personal fact</p>
        <label>wording <input bind:value={claim} placeholder="{'{name}'} uses drugs" /></label>
        <button class="primary" type="submit">add fact</button>
      </form>
    {/if}
  </div>
</div>
