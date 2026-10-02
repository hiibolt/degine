<script>
  import { dismiss, toasts } from "./toast.svelte.js";
</script>

<div class="toasts">
  {#each toasts as toast (toast.id)}
    <button class="toast {toast.tone}" type="button" onclick={() => dismiss(toast.id)}>
      {toast.message}
    </button>
  {/each}
</div>
