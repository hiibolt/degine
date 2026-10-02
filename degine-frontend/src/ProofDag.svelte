<script>
  import dagre from "@dagrejs/dagre";
  import { Background, BackgroundVariant, Controls, MarkerType, SvelteFlow } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import DagNode from "./DagNode.svelte";
  import FlowFit from "./FlowFit.svelte";
  import Icon from "./Icon.svelte";

  let { tree, onopen, unfold = false } = $props();

  const nodeTypes = { card: DagNode };

  function opened(node, path, into) {
    if (node.steps.length || node.assumed.length) into.add(path);
    node.steps.forEach((step, index) => {
      step.needs.forEach((need, child) => opened(need, `${path}/${index}/${child}`, into));
    });
    node.assumed.forEach((extra, child) => opened(extra, `${path}/a/${child}`, into));
    return into;
  }

  let expanded = $state(unfold ? opened(tree, "root", new Set()) : new Set());
  let focus = $state("root");
  let framed = $state("root");
  let nodes = $state.raw([]);
  let edges = $state.raw([]);
  let host = $state(null);
  let covered = $state(false);
  let native = $state(false);
  let rev = $state(0);
  const full = $derived(covered || native);

  function byKind(step) {
    if (step.needs.length < 2) return "by";
    return step.join === "or" ? "by any" : "by all";
  }

  function badge(node) {
    if (node.cycle) return "loop";
    if (node.role === "fact" || node.role === "criterion" || node.role === "assumed") return node.role;
    if (!node.steps.length && !node.assumed.length) return "open";
    return "";
  }

  function box(title, gate, meta) {
    const text = title || "";
    const width = gate
      ? text
        ? Math.min(268, Math.max(196, Math.round(text.length * 6.6)))
        : 118
      : Math.min(260, Math.max(168, Math.round(text.length * 8 + 62)));
    const per = Math.max(16, Math.floor((width - (gate ? 36 : 58)) / 7.2));
    const lines = Math.max(1, Math.ceil((text || " ").length / per));
    const height = gate ? 16 + 24 + (text ? lines * 18 : 0) : 16 + lines * 20 + (meta ? 22 : 0);
    return { width, height };
  }

  function card(id, data) {
    const size = box(data.title, data.gate, Boolean(data.badge || data.library));
    return {
      id,
      type: "card",
      data: { ...data, ...size, ontoggle: toggle, onopen },
      position: { x: 0, y: 0 },
      style: `width:${size.width}px`,
    };
  }

  function walk(node, path, open, built, links) {
    const canOpen = !node.cycle && (node.steps.length > 0 || node.assumed.length > 0);
    built.push(
      card(path, {
        gate: false,
        path,
        title: node.title,
        badge: badge(node),
        library: node.library,
        factId: node.library ? node.id : "",
        canOpen,
        open: open.has(path),
      }),
    );
    if (!open.has(path)) return;

    node.steps.forEach((step, index) => {
      const gate = `${path}/w${index}`;
      built.push(
        card(gate, {
          gate: true,
          path: gate,
          kind: byKind(step),
          title: step.theoremTitle,
          factId: step.theoremTitle ? step.theoremId : "",
          canOpen: false,
          open: false,
          library: false,
          badge: "",
        }),
      );
      links.push({ parent: path, child: gate });
      step.needs.forEach((need, child) => {
        const id = `${path}/${index}/${child}`;
        links.push({ parent: gate, child: id });
        walk(need, id, open, built, links);
      });
    });

    if (!node.assumed.length) return;
    const gate = `${path}/assumed`;
    built.push(
      card(gate, {
        gate: true,
        path: gate,
        kind: "assumed",
        title: "",
        factId: "",
        canOpen: false,
        open: false,
        library: false,
        badge: "",
      }),
    );
    links.push({ parent: path, child: gate });
    node.assumed.forEach((extra, child) => {
      const id = `${path}/a/${child}`;
      links.push({ parent: gate, child: id });
      walk(extra, id, open, built, links);
    });
  }

  function place(built, links) {
    const graph = new dagre.graphlib.Graph();
    graph.setDefaultEdgeLabel(() => ({}));
    graph.setGraph({ rankdir: "TB", nodesep: 32, ranksep: 52, marginx: 20, marginy: 20 });
    for (const node of built) graph.setNode(node.id, { width: node.data.width, height: node.data.height });
    for (const edge of links) graph.setEdge(edge.parent, edge.child);
    dagre.layout(graph);
    return built.map((node) => {
      const laid = graph.node(node.id);
      return {
        ...node,
        position: {
          x: laid.x - node.data.width / 2,
          y: laid.y - node.data.height / 2,
        },
      };
    });
  }

  function nearby(path, links) {
    const ids = new Set([path]);
    for (const edge of links) {
      if (edge.parent !== path) continue;
      ids.add(edge.child);
      for (const hop of links) if (hop.parent === edge.child) ids.add(hop.child);
    }
    return [...ids];
  }

  async function toggleFull() {
    if (full) {
      covered = false;
      rev += 1;
      if (document.fullscreenElement || document.webkitFullscreenElement) {
        const leave = document.exitFullscreen?.bind(document) || document.webkitExitFullscreen?.bind(document);
        if (leave) await leave().catch(() => {});
      }
      return;
    }
    const ask = host.requestFullscreen?.bind(host) || host.webkitRequestFullscreen?.bind(host);
    try {
      if (!ask) throw new Error("fullscreen unavailable");
      await ask();
    } catch {
      covered = true;
      rev += 1;
    }
  }

  $effect(() => {
    if (!covered) return;
    const onKey = (event) => {
      if (event.key !== "Escape") return;
      covered = false;
      rev += 1;
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  $effect(() => {
    const sync = () => {
      const on = document.fullscreenElement === host;
      if (on === native) return;
      native = on;
      rev += 1;
    };
    document.addEventListener("fullscreenchange", sync);
    return () => document.removeEventListener("fullscreenchange", sync);
  });

  function toggle(path) {
    const next = new Set(expanded);
    if (next.has(path)) {
      for (const key of next) {
        if (key === path || key.startsWith(`${path}/`)) next.delete(key);
      }
    } else next.add(path);
    focus = path;
    expanded = next;
  }

  $effect(() => {
    const open = expanded;
    const target = focus;
    const root = tree;
    if (!root) {
      nodes = [];
      edges = [];
      framed = "";
      return;
    }
    const built = [];
    const links = [];
    walk(root, "root", open, built, links);
    nodes = place(built, links);
    edges = links.map((edge) => ({
      id: `${edge.child}>${edge.parent}`,
      source: edge.child,
      target: edge.parent,
      type: "smoothstep",
    }));
    const ids = nearby(target, links).filter((id) => built.some((node) => node.id === id));
    framed = (ids.length ? ids : ["root"]).join(",");
  });
</script>

<div class="proof-dag" class:cover={covered} bind:this={host}>
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    nodesDraggable={false}
    nodesConnectable={false}
    elementsSelectable={false}
    deleteKey={null}
    colorMode="light"
    minZoom={0.2}
    maxZoom={1.75}
    proOptions={{ hideAttribution: true }}
    defaultEdgeOptions={{
      type: "smoothstep",
      style: "stroke:#c4a48a;stroke-width:1.6",
      markerEnd: { type: MarkerType.ArrowClosed, color: "#c4a48a", width: 16, height: 16 },
    }}
  >
    <Background variant={BackgroundVariant.Dots} gap={18} size={1.3} bgColor="#fbf7f1" patternColor="#e6d5c4" />
    <Controls showLock={false} />
    <FlowFit stamp={framed} {rev} />
  </SvelteFlow>
  <button
    class="icon dag-full"
    type="button"
    aria-label={full ? "leave fullscreen" : "fullscreen"}
    title={full ? "leave fullscreen" : "fullscreen"}
    onpointerdown={(event) => event.stopPropagation()}
    onclick={toggleFull}
  >
    <Icon name={full ? "compress" : "expand"} />
  </button>
</div>
