<script>
  import Icon from "./Icon.svelte";

  let { join = "and", parts = [], thenTitle = "", onopen = null } = $props();
</script>

<div class="claim-view">
  <div class="claim-line">
    <span class="kicker">if</span>
    {#if parts.length > 1}
      <span class="join-word">{join === "or" ? "any" : "all"}</span>
    {/if}
    {#each parts as part, index (`${index}-${part}`)}
      {#if onopen && part.id}
        <button class="chip" class:not={part.not} type="button" onclick={() => onopen(part.id)}>
          {#if part.not}<Icon name="x" />{/if}
          {part.title}
        </button>
      {:else}
        <span class="chip" class:not={part.not}>
          {#if part.not}<Icon name="x" />{/if}
          {part.title || part}
        </span>
      {/if}
    {/each}
  </div>
  <div class="claim-line">
    <span class="kicker">then</span>
    {#if onopen && thenTitle?.id}
      <button class="chip then" type="button" onclick={() => onopen(thenTitle.id)}>{thenTitle.title}</button>
    {:else}
      <span class="chip then">{thenTitle?.title || thenTitle}</span>
    {/if}
  </div>
</div>
