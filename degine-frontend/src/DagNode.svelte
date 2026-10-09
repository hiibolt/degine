<script>
  import { Handle, Position } from "@xyflow/svelte";
  import Icon from "./Icon.svelte";

  let { data } = $props();

  function toggle(event) {
    event.stopPropagation();
    data.ontoggle(data.path);
  }

  function open(event) {
    event.stopPropagation();
    data.onopen(data.factId);
  }

  function keep(event) {
    event.stopPropagation();
  }

  function jump(event) {
    if (!data.alias) return;
    event.stopPropagation();
    data.ontoggle(data.path);
  }
</script>

<div
  class="dag-card"
  class:gate={data.gate}
  class:dim={data.dim}
  class:alias={Boolean(data.alias)}
  class:not={data.not}
  style="width:{data.width}px;min-height:{data.height}px"
  onclick={jump}
>
  <Handle type="source" position={Position.Top} />
  {#if data.gate}
    <span class="proof-role">{data.kind}</span>
    {#if data.title}<span class="dag-title">{data.title}</span>{/if}
    {#if data.factId}
      <button class="icon view nodrag nopan" type="button" aria-label="view" title="view" onpointerdown={keep} onclick={open}>
        <Icon name="eye" />
      </button>
    {/if}
  {:else}
    <div class="dag-claim">
      {#if data.canOpen}
        <button
          class="twist nodrag nopan"
          type="button"
          aria-expanded={data.open}
          aria-label={data.alias ? "show the open one" : data.open ? "collapse" : "expand"}
          onclick={toggle}
        >
          {data.open ? "−" : "+"}
        </button>
      {:else}
        <span class="twist quiet-mark" aria-hidden="true"></span>
      {/if}
      <strong class="dag-title">
        {#if data.not}<Icon name="x" />{/if}
        {data.title}
      </strong>
    </div>
    {#if data.badge || data.library}
      <div class="dag-meta">
        {#if data.badge}<span class="proof-role">{data.badge}</span>{/if}
        {#if data.library}
          <button class="icon view nodrag nopan" type="button" aria-label="view" title="view" onpointerdown={keep} onclick={open}>
            <Icon name="eye" />
          </button>
        {/if}
      </div>
    {/if}
  {/if}
  <Handle type="target" position={Position.Bottom} />
</div>
