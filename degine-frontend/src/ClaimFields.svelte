<script>
  import Icon from "./Icon.svelte";
  import Pick from "./Pick.svelte";

  let {
    join = $bindable("and"),
    parts = $bindable([""]),
    thenId = $bindable(""),
    thenName = $bindable(""),
    mintThen = false,
    options = [],
  } = $props();

  function add() {
    parts = [...parts, ""];
  }

  function remove(index) {
    parts = parts.filter((_, item) => item !== index);
    if (!parts.length) parts = [""];
  }
</script>

<div class="claim-fields">
  <div class="row">
    <span class="kicker plain">if</span>
    <select bind:value={join}>
      <option value="and">all of</option>
      <option value="or">any of</option>
    </select>
  </div>
  {#each parts as part, index (index)}
    <div class="row">
      <Pick bind:value={parts[index]} {options} />
      <button class="icon quiet" type="button" aria-label="remove" title="remove" onclick={() => remove(index)}>
        <Icon name="x" />
      </button>
    </div>
  {/each}
  <button class="icon" type="button" aria-label="add another" title="add another" onclick={add}>
    <Icon name="plus" />
  </button>
  <label>
    then
    <Pick bind:value={thenId} bind:name={thenName} mint={mintThen} placeholder={mintThen ? "" : "search"} {options} />
  </label>
</div>
