function nodeKey(node) {
  if (!node?.id) return "";
  return node.not ? `not:${node.id}` : node.id;
}

function descendants(node, into = new Set()) {
  const key = nodeKey(node);
  if (key) into.add(key);
  for (const step of node.steps || []) {
    for (const need of step.needs) descendants(need, into);
  }
  for (const extra of node.assumed || []) descendants(extra, into);
  return into;
}

export function ancestors(path) {
  const found = [];
  let current = path;
  while (current && current !== "root") {
    const bits = current.split("/");
    if (bits.length < 3) break;
    bits.splice(bits.length - 2, 2);
    current = bits.join("/");
    found.push(current);
  }
  return found;
}

function visit(node, path, into) {
  into.push({
    path,
    key: nodeKey(node),
    desc: descendants(node),
    expandable: !node.cycle && ((node.steps || []).length > 0 || (node.assumed || []).length > 0),
  });
  (node.steps || []).forEach((step, index) => {
    step.needs.forEach((need, child) => visit(need, `${path}/${index}/${child}`, into));
  });
  (node.assumed || []).forEach((extra, child) => visit(extra, `${path}/a/${child}`, into));
}

// One expanded copy per fact. A second copy stays expanded only when it
// still contains a descendant the copies already kept do not.
export function layoutPlan(root) {
  if (!root) return { initial: new Set(), homes: new Map(), stamp: "" };
  const occ = [];
  visit(root, "root", occ);
  const groups = new Map();
  for (const item of occ) {
    if (!item.key) continue;
    const list = groups.get(item.key);
    if (list) list.push(item);
    else groups.set(item.key, [item]);
  }
  const homes = new Map();
  const kept = new Set();
  for (const items of groups.values()) {
    const universe = new Set();
    for (const item of items) for (const id of item.desc) universe.add(id);
    const covered = new Set();
    const picked = [];
    while (covered.size < universe.size) {
      let best = null;
      let bestGain = 0;
      for (const item of items) {
        if (picked.includes(item)) continue;
        let gain = 0;
        for (const id of item.desc) if (!covered.has(id)) gain += 1;
        if (gain > bestGain) {
          bestGain = gain;
          best = item;
        }
      }
      if (!best) break;
      picked.push(best);
      for (const id of best.desc) covered.add(id);
    }
    if (!picked.length) picked.push(items[0]);
    const home = picked[0].path;
    for (const item of picked) kept.add(item.path);
    for (const item of items) if (!picked.includes(item)) homes.set(item.path, home);
  }
  const initial = new Set();
  for (const item of occ) {
    if (item.expandable && !homes.has(item.path)) initial.add(item.path);
  }
  for (const path of [...initial]) for (const up of ancestors(path)) initial.add(up);
  const stamp = occ.map((item) => `${item.path}=${item.key}:${homes.get(item.path) || ""}`).join("|");
  return { initial, homes, stamp };
}
