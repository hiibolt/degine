<script>
  let { value = $bindable(""), name = $bindable(""), options = [], mint = false, placeholder = "search" } = $props();

  let text = $state("");
  let open = $state(false);
  let writing = $state(false);

  const chosen = $derived(
    options.find((item) => item.id === value && item.role !== "theorem") ||
      options.find((item) => item.id === value),
  );
  const matches = $derived.by(() => {
    const needle = writing ? text.trim().toLowerCase() : "";
    const rows = options.filter((item) => !needle || item.hay.includes(needle)).slice(0, 8);
    const typed = text.trim();
    const exact = options.some(
      (item) => item.role !== "theorem" && item.title.toLowerCase() === typed.toLowerCase(),
    );
    const outcome = options.find((item) => item.id === value && item.role === "outcome");
    if (mint && writing && typed && !exact) {
      if (outcome) rows.unshift({ rename: true, id: outcome.id, title: typed, role: "outcome", hay: "" });
      else rows.push({ fresh: true, id: "", title: typed, role: "", hay: "" });
    }
    return rows;
  });

  function atomTitle(id, fallback) {
    return options.find((row) => row.id === id && row.role !== "theorem")?.title || fallback || "";
  }

  $effect(() => {
    if (writing) return;
    text = mint ? name : chosen?.title || "";
  });

  function choose(item) {
    if (item.fresh) {
      value = "";
      name = item.title;
      text = item.title;
    } else if (item.rename) {
      value = item.id;
      name = item.title;
      text = item.title;
    } else {
      value = item.id;
      const title = atomTitle(item.id, item.title);
      if (mint) name = title;
      text = title;
    }
    writing = false;
    open = false;
  }

  function commit() {
    const typed = text.trim();
    if (!mint) {
      text = chosen?.title || "";
      return;
    }
    const exact = options.find(
      (item) => item.role !== "theorem" && item.title.toLowerCase() === typed.toLowerCase(),
    );
    if (exact) {
      value = exact.id;
      name = exact.title;
    } else if (!typed) {
      value = "";
      name = "";
    } else if (value && options.some((item) => item.id === value && item.role === "outcome")) {
      name = typed;
    } else {
      value = "";
      name = typed;
    }
    text = name;
  }

  function blur(event) {
    if (event.currentTarget.contains(event.relatedTarget)) return;
    writing = false;
    open = false;
    commit();
  }
</script>

<div class="pick-box" onfocusout={blur}>
  <input
    value={text}
    {placeholder}
    autocomplete="off"
    role="combobox"
    aria-expanded={open}
    onfocus={() => (open = true)}
    oninput={(event) => {
      writing = true;
      text = event.currentTarget.value;
      open = true;
    }}
    onkeydown={(event) => {
      if (event.key === "Enter" && matches[0]) {
        event.preventDefault();
        choose(matches[0]);
      }
      if (event.key === "Escape") {
        writing = false;
        open = false;
        text = mint ? name : chosen?.title || "";
      }
    }}
  />
  {#if open}
    <ul class="pick-menu">
      {#each matches as item (`${item.role}:${item.id}:${item.title}`)}
        <li>
          <button
            type="button"
            onmousedown={(event) => event.preventDefault()}
            onclick={() => choose(item)}
          >
            <span>{item.title}</span>
            {#if item.fresh}<span class="proof-role">+</span>{:else if item.role}<span class="proof-role">{item.role}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
