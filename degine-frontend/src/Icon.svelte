<script>
  let { name } = $props();
</script>

<svg viewBox="0 0 24 24" aria-hidden="true">
  {#if name === "plus"}
    <path d="M12 5v14M5 12h14" />
  {:else if name === "pencil"}
    <path d="M4 20h4l10-10-4-4L4 16zM13 7l4 4" />
  {:else if name === "eye"}
    <path d="M2 12s4-6 10-6 10 6 10 6-4 6-10 6S2 12 2 12z" />
    <circle cx="12" cy="12" r="2.5" />
  {:else if name === "trash"}
    <path d="M5 7h14M9 7V5h6v2M8 7l1 12h6l1-12" />
  {:else if name === "check"}
    <path d="M5 12.5 10 17l9-10" />
  {:else if name === "x"}
    <path d="M6 6l12 12M18 6 6 18" />
  {:else if name === "search"}
    <circle cx="11" cy="11" r="6" />
    <path d="M16 16l4 4" />
  {:else if name === "expand"}
    <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
  {:else if name === "compress"}
    <path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" />
  {:else if name === "inbox"}
    <path d="M4 13h4l1.5 2h5L16 13h4" />
    <path d="M4 13 6 5h12l2 8v6H4z" />
  {:else if name === "share"}
    <path d="M12 3v10" />
    <path d="M8 7l4-4 4 4" />
    <path d="M5 13v6h14v-6" />
  {/if}
</svg>
