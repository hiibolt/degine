<script>
  import Icon from "./Icon.svelte";

  let { thread = [], username, canWrite, onadd, onedit, onremove, onresolve } = $props();

  let draft = $state("");
  let editingId = $state(null);
  let editBody = $state("");

  const open = $derived(thread.filter((comment) => !comment.resolved));
  const archived = $derived(thread.filter((comment) => comment.resolved));

  function canDelete(comment) {
    return comment.author === username || canWrite;
  }

  async function send(event) {
    event.preventDefault();
    const body = draft.trim();
    if (!body) return;
    await onadd(body);
    draft = "";
  }

  function startEdit(comment) {
    editingId = comment.id;
    editBody = comment.body;
  }

  async function saveEdit(event) {
    event.preventDefault();
    const body = editBody.trim();
    if (!body || editingId == null) return;
    await onedit(editingId, body);
    editingId = null;
  }

  async function remove(comment) {
    if (!confirm("delete this comment?")) return;
    await onremove(comment.id);
  }
</script>

<section class="comments">
  <h3>comments</h3>
  {#each open as comment (comment.id)}
    <article>
      <p class="kicker">{comment.author}</p>
      {#if editingId === comment.id}
        <form class="stack" onsubmit={saveEdit}>
          <textarea bind:value={editBody}></textarea>
          <div class="actions">
            <button class="icon primary" type="submit" aria-label="save" title="save"><Icon name="check" /></button>
            <button class="icon quiet" type="button" aria-label="cancel" title="cancel" onclick={() => (editingId = null)}>
              <Icon name="x" />
            </button>
          </div>
        </form>
      {:else}
        <p>{comment.body}</p>
        <div class="actions">
          {#if comment.author === username}
            <button class="icon" type="button" aria-label="edit" title="edit" onclick={() => startEdit(comment)}>
              <Icon name="pencil" />
            </button>
          {/if}
          {#if canDelete(comment)}
            <button class="icon" type="button" aria-label="resolve" title="resolve" onclick={() => onresolve(comment.id, true)}>
              <Icon name="check" />
            </button>
            <button class="icon danger" type="button" aria-label="delete" title="delete" onclick={() => remove(comment)}>
              <Icon name="trash" />
            </button>
          {/if}
        </div>
      {/if}
    </article>
  {/each}
  <form class="stack" onsubmit={send}>
    <textarea bind:value={draft}></textarea>
    <button class="icon" type="submit" aria-label="comment" title="comment"><Icon name="plus" /></button>
  </form>
  {#if archived.length}
    <details class="archive">
      <summary>archive</summary>
      {#each archived as comment (comment.id)}
        <article>
          <p class="kicker">{comment.author}</p>
          <p>{comment.body}</p>
          {#if canDelete(comment)}
            <div class="actions">
              <button class="quiet" type="button" onclick={() => onresolve(comment.id, false)}>unarchive</button>
              <button class="icon danger" type="button" aria-label="delete" title="delete" onclick={() => remove(comment)}>
                <Icon name="trash" />
              </button>
            </div>
          {/if}
        </article>
      {/each}
    </details>
  {/if}
</section>
