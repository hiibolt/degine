<script>
  import FormulaView from "./FormulaView.svelte";

  let { node, facts = [], titleOf = (id) => id, onpick = () => {} } = $props();

  const fact = $derived(
    node?.kind === "atom" ? facts.find((item) => item.id === node.id && !item.formula) : null,
  );
  const label = $derived(node?.kind === "atom" ? titleOf(node.id) : "");
</script>

{#if node.kind === "imp"}
  <div class="flow">
    <div class="pane">
      <span class="kicker">if</span>
      <FormulaView node={node.args[0]} {facts} {titleOf} {onpick} />
    </div>
    <div class="bridge" aria-hidden="true">→</div>
    <div class="pane then">
      <span class="kicker">then</span>
      <FormulaView node={node.args[1]} {facts} {titleOf} {onpick} />
    </div>
  </div>
{:else if node.kind === "and" || node.kind === "or"}
  <div class="bundle">
    {#each node.args as arg, index (`${node.kind}-${index}`)}
      {#if index}
        <span class="join">{node.kind}</span>
      {/if}
      <FormulaView node={arg} {facts} {titleOf} {onpick} />
    {/each}
  </div>
{:else if node.kind === "not"}
  <div class="negation">
    <span class="kicker">not</span>
    <FormulaView node={node.args[0]} {facts} {titleOf} {onpick} />
  </div>
{:else if node.kind === "true" || node.kind === "false"}
  <div class="atom constant">{node.kind}</div>
{:else if node.kind === "atom"}
  {#if fact}
    <button class="atom cited" type="button" onclick={() => onpick(fact.id)}>
      <span class="atom-id">{label}</span>
    </button>
  {:else}
    <div class="atom input">
      <span class="role">input</span>
      <span class="atom-id">{label}</span>
    </div>
  {/if}
{/if}
