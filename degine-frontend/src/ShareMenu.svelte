<script>
  import Icon from "./Icon.svelte";

  let { owner, shares = [], canExport = false, onexport, ontoggle, onload } = $props();

  let open = $state(false);
  let email = $state("");

  async function toggle() {
    open = !open;
    if (open && owner) await onload();
  }

  function add(event) {
    event.preventDefault();
    const value = email.trim().toLowerCase();
    if (!value.includes("@")) return;
    ontoggle(value, true);
    email = "";
  }
</script>

<div class="pop">
  <button class="icon" type="button" aria-label="share" title="share" onclick={toggle}>
    <Icon name="share" />
  </button>
  {#if open}
    <div class="menu">
      {#if canExport}
        <button class="quiet" type="button" onclick={onexport}>export</button>
      {/if}
      {#if owner}
        <form class="share-add" onsubmit={add}>
          <input bind:value={email} type="email" placeholder="email" autocomplete="off" />
          <button class="quiet" type="submit">add</button>
        </form>
        {#each shares as person}
          <div class="share-row">
            <span>{person}</span>
            <button class="quiet" type="button" onclick={() => ontoggle(person, false)}>remove</button>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</div>
