<script>
  import Icon from "./Icon.svelte";

  let { items = [], onopen } = $props();

  let open = $state(false);
</script>

<div class="pop">
  <button class="icon" type="button" aria-label="inbox" title="inbox" onclick={() => (open = !open)}>
    <Icon name="inbox" />
    {#if items.length}<span class="count">{items.length}</span>{/if}
  </button>
  {#if open}
    <div class="menu">
      {#each items as comment (comment.id)}
        <button
          class="inbox-row"
          type="button"
          onclick={() => {
            open = false;
            onopen(comment);
          }}
        >
          <strong>{comment.author}</strong>
          <span>{comment.body}</span>
        </button>
      {:else}
        <p class="empty">quiet</p>
      {/each}
    </div>
  {/if}
</div>
