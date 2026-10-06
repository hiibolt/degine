<script>
  import { fillName, fillPeople } from "./phrases.js";

  let {
    people = [],
    personal = [],
    canWrite = false,
    onaddPerson,
    onremovePerson,
    onaddFact,
    ontoggle,
    onlink,
    onerror,
    onhere = () => {},
    looking = [],
  } = $props();

  let personId = $state("");
  let name = $state("");
  let factId = $state("");
  let claim = $state("");
  let openFact = $state("");
  let picked = $state("");
  let armed = $state("");

  const person = $derived(people.find((item) => item.id === personId) || people[0] || null);
  const listedPeople = $derived([...people].sort((a, b) => a.name.localeCompare(b.name)));
  const listedFacts = $derived([...personal].sort((a, b) => a.claim.localeCompare(b.claim)));
  const on = $derived(new Set(person?.on || []));
  const others = $derived(listedPeople.filter((item) => item.id !== person?.id));

  function chosen(factId) {
    const ids = new Set((person?.links || []).filter((link) => link.fact === factId).map((link) => link.other));
    return others.filter((item) => ids.has(item.id));
  }

  function leftOut(factId) {
    const ids = new Set(chosen(factId).map((item) => item.id));
    return others.filter((item) => !ids.has(item.id));
  }

  async function confirmLink(fact) {
    const other = others.find((item) => item.id === picked);
    if (!other || !person) return;
    const key = `${fact.id}:${other.id}`;
    if (armed !== key) {
      armed = key;
      return;
    }
    try {
      await onlink(person.id, fact.id, other.id, true);
      openFact = "";
      picked = "";
      armed = "";
    } catch (err) {
      onerror(err);
    }
  }

  async function dropLink(fact, other) {
    const key = `off:${fact.id}:${other.id}`;
    if (armed !== key) {
      armed = key;
      return;
    }
    try {
      await onlink(person.id, fact.id, other.id, false);
      armed = "";
    } catch (err) {
      onerror(err);
    }
  }

  $effect(() => {
    onhere(person?.id || "");
  });

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
          <span>{item.name}</span>
          <span class="peers">
            {#each looking.filter((face) => face.tab === "people" && face.item === item.id) as face (face.user_id)}
              <span class="peer" style:background={face.color} title={face.label}></span>
            {/each}
          </span>
        </button>
      {/each}
    </div>
    {#if canWrite}
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
        {#if canWrite}
          <button class="quiet" type="button" onclick={() => onremovePerson(person.id)}>remove</button>
        {/if}
      </div>
      <ul class="toggles">
        {#each listedFacts as fact (fact.id)}
          <li>
            {#if fact.claim.includes("{other}")}
              <div class="fact-toggle rel">
                <span>{fillPeople(fact.claim, person.name, "someone")}</span>
                <div class="who">
                  {#each chosen(fact.id) as other (other.id)}
                    {#if canWrite}
                      <button type="button" class="on" onclick={() => dropLink(fact, other)}>
                        {armed === `off:${fact.id}:${other.id}` ? `drop ${other.name}?` : other.name}
                      </button>
                    {:else}
                      <button type="button" class="on" disabled>{other.name}</button>
                    {/if}
                  {/each}
                  {#if canWrite}
                    {#if openFact === fact.id}
                      <select bind:value={picked} onchange={() => (armed = "")}>
                        <option value="">pick someone</option>
                        {#each leftOut(fact.id) as other (other.id)}
                          <option value={other.id}>{other.name}</option>
                        {/each}
                      </select>
                      <button type="button" disabled={!picked} onclick={() => confirmLink(fact)}>
                        {armed === `${fact.id}:${picked}` ? "yes, add them" : "add"}
                      </button>
                    {:else}
                      <button type="button" onclick={() => { openFact = fact.id; picked = ""; armed = ""; }}>+</button>
                    {/if}
                  {/if}
                </div>
              </div>
            {:else}
              <label class="fact-toggle">
                <input
                  type="checkbox"
                  checked={on.has(fact.id)}
                  disabled={!canWrite}
                  onchange={(event) => ontoggle(person.id, fact.id, event.currentTarget.checked)}
                />
                <span>{fillName(fact.claim, person.name)}</span>
              </label>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p class="dek">No people yet.</p>
    {/if}
    {#if canWrite}
      <form class="stack add-fact" onsubmit={addFact}>
        <p class="kicker">new personal fact</p>
        <label>wording, use {'{name}'} and {'{other}'} for a person picker <input bind:value={claim} placeholder="{'{name}'} loves {'{other}'}" /></label>
        <button class="primary" type="submit">add fact</button>
      </form>
    {/if}
  </div>
</div>
