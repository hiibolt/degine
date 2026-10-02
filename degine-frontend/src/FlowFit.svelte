<script>
  import { tick } from "svelte";
  import { useSvelteFlow } from "@xyflow/svelte";

  let { stamp, rev = 0 } = $props();
  const flow = useSvelteFlow();

  $effect(() => {
    rev;
    const ids = String(stamp || "")
      .split(",")
      .filter(Boolean)
      .map((id) => ({ id }));
    if (!ids.length) return;
    let cancel = false;
    (async () => {
      await tick();
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      if (!cancel) flow.fitView({ nodes: ids, padding: 0.22, duration: 180, maxZoom: 1 });
    })();
    return () => {
      cancel = true;
    };
  });
</script>
