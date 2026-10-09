import { readClaim } from "./phrases.js";

function roleOf(fact) {
  if (!fact) return "outcome";
  if (fact.role === "fact" || fact.role === "criterion" || fact.role === "theorem") return fact.role;
  return fact.formula ? "theorem" : "fact";
}

function leaf(id, role, titleOf) {
  return {
    id,
    title: titleOf(id),
    role,
    library: role === "fact" || role === "criterion",
    cycle: false,
    steps: [],
    assumed: [],
  };
}

function shorter(way, other) {
  return other.assumes.length < way.assumes.length && other.assumes.every((id) => way.assumes.includes(id));
}

function prune(ways) {
  return ways.filter((way) => !ways.some((other) => other !== way && shorter(way, other)));
}

function product(lists) {
  let rows = [[]];
  for (const list of lists) {
    const sorted = [...list].sort((a, b) => a.assumes.length - b.assumes.length);
    const next = [];
    for (const row of rows) {
      for (const item of sorted) {
        next.push([...row, item]);
        if (next.length >= 80) break;
      }
      if (next.length >= 80) break;
    }
    rows = next;
  }
  return rows;
}

function merge(parts) {
  const assumes = [];
  const givens = [];
  for (const part of parts) {
    for (const id of part.assumes) if (!assumes.includes(id)) assumes.push(id);
    for (const id of part.givens) if (!givens.includes(id)) givens.push(id);
  }
  return { assumes, givens };
}

function waysFrom(id, facts, titleOf, stack) {
  if (id.startsWith("not:")) {
    const inner = id.slice(4);
    if (roleOf(facts.find((item) => item.id === inner)) === "fact") return [];
    const tree = leaf(inner, "criterion", titleOf);
    tree.not = true;
    return [{ assumes: [id], givens: [], tree }];
  }
  if (stack.has(id) || stack.size > 16) return [];
  const next = new Set(stack);
  next.add(id);
  const fact = facts.find((item) => item.id === id);
  const role = roleOf(fact);
  const found = [];
  if (role === "fact") found.push({ assumes: [], givens: [id], tree: leaf(id, "fact", titleOf) });
  for (const item of facts) {
    if (roleOf(item) !== "theorem" || !item.formula) continue;
    const claim = readClaim(item.formula);
    if (!claim || claim.thenId !== id) continue;
    const groups =
      claim.join === "or"
        ? claim.partIds.map((part) => waysFrom(part, facts, titleOf, next))
        : [null];
    const bundles =
      claim.join === "or"
        ? groups.filter((list) => list.length).map((list) => list.map((way) => [way]))
        : [product(claim.partIds.map((part) => waysFrom(part, facts, titleOf, next)))];
    for (const bundle of bundles.flat()) {
      if (!bundle.length) continue;
      const joined = merge(bundle);
      found.push({
        assumes: joined.assumes,
        givens: joined.givens,
        tree: {
          id,
          title: titleOf(id),
          role: "step",
          library: false,
          cycle: false,
          assumed: [],
          steps: [
            {
              theoremId: item.id,
              theoremTitle: item.claim,
              join: claim.join === "or" ? "or" : "and",
              needs: bundle.map((way) => way.tree),
            },
          ],
        },
      });
    }
  }
  if (role === "criterion") found.push({ assumes: [id], givens: [], tree: leaf(id, "criterion", titleOf) });
  return prune(found);
}

function keyOf(way) {
  const steps = [];
  const walk = (node) => {
    for (const step of node.steps) {
      steps.push(step.theoremId);
      for (const need of step.needs) walk(need);
    }
  };
  walk(way.tree);
  return `${[...way.assumes].sort().join(",")}|${steps.join(">")}`;
}

function mergeTrees(nodes) {
  const first = nodes[0];
  if (nodes.length === 1) return first;
  const order = [];
  const groups = new Map();
  for (const node of nodes) {
    for (const step of node.steps || []) {
      let bucket = groups.get(step.theoremId);
      if (!bucket) {
        bucket = [];
        groups.set(step.theoremId, bucket);
        order.push(step);
      }
      bucket.push(step);
    }
  }
  const steps = order.map((sample) => {
    const bucket = groups.get(sample.theoremId);
    const width = Math.max(...bucket.map((step) => step.needs.length));
    const needs = [];
    for (let index = 0; index < width; index++) {
      const kids = bucket.map((step) => step.needs[index]).filter(Boolean);
      needs.push(kids.length === 1 ? kids[0] : mergeTrees(kids));
    }
    return { ...sample, needs };
  });
  return { ...first, steps };
}

// Several chains can leave the same criteria. The card only prints those, so show one row
// and keep every route as another step in that proof.
function sameCase(ways) {
  const groups = new Map();
  for (const way of ways) {
    const key = [...way.assumes].sort().join(",");
    const list = groups.get(key);
    if (list) list.push(way);
    else groups.set(key, [way]);
  }
  return [...groups.values()].map((group) => {
    const givens = [];
    for (const way of group) for (const id of way.givens) if (!givens.includes(id)) givens.push(id);
    const way = {
      assumes: group[0].assumes,
      givens,
      tree: mergeTrees(group.map((item) => item.tree)),
    };
    return { ...way, key: keyOf(way), shown: way.assumes.length ? way.assumes : way.givens };
  });
}

let cachedKey = "";
let cachedBoard = [];

function boardKey(facts, labels) {
  const lines = facts.map((fact) => `${fact.id}|${fact.role}|${fact.formula || ""}|${fact.claim}`);
  lines.sort();
  const names = Object.keys(labels || {}).sort();
  for (const id of names) lines.push(`${id}=${labels[id]}`);
  return lines.join("\n");
}

export function personFacts(facts, personal, person, people = []) {
  const on = new Set(person?.on || []);
  const links = person?.links || [];
  const taken = new Set((facts || []).map((fact) => fact.id));
  const extra = [];
  for (const item of personal || []) {
    if (String(item.claim || "").includes("{other}")) {
      if (!person) continue;
      for (const other of people) {
        if (other.id === person.id) continue;
        const id = `${item.id}__${other.id}`;
        if (taken.has(id)) continue;
        const chosen = links.some((link) => link.fact === item.id && link.other === other.id);
        if (!chosen) continue;
        taken.add(id);
        extra.push({
          id,
          claim: item.claim.replaceAll("{name}", person.name).replaceAll("{other}", other.name),
          role: "fact",
          formula: null,
        });
      }
      continue;
    }
    if (person && on.has(item.id) && !taken.has(item.id)) {
      extra.push({
        id: item.id,
        claim: item.claim,
        role: "fact",
        formula: null,
      });
    }
  }
  return [...facts, ...extra];
}

// Proved when a chain reaches the atom with nothing left to assume.
// Null when the formula is not a single fact, so the caller can fall back.
export function holds(facts, formula) {
  const bare = String(formula || "").trim().match(/^fact:([A-Za-z0-9_]+)$/);
  if (!bare) return null;
  const ways = waysFrom(bare[1], facts, (id) => id, new Set());
  if (ways.some((way) => !way.assumes.length)) return "proved";
  if (ways.length) return "open";
  return "invalid";
}

// A named person does not stay open just because some other criterion could be assumed.
export function holdsForPerson(facts, formula) {
  const status = holds(facts, formula);
  return status === "open" ? "invalid" : status;
}

export function outcomeBoard(facts, labels, titleOf) {
  const key = boardKey(facts, labels);
  if (key === cachedKey) return cachedBoard;
  const board = buildBoard(facts, labels, titleOf);
  cachedKey = key;
  cachedBoard = board;
  return board;
}

function buildBoard(facts, labels, titleOf) {
  const taken = new Set(facts.map((fact) => fact.id));
  const ids = Object.keys(labels || {}).filter((id) => !taken.has(id));
  return ids
    .map((id) => {
      const seen = new Set();
      const found = [];
      for (const way of waysFrom(id, facts, titleOf, new Set())) {
        const key = keyOf(way);
        if (seen.has(key)) continue;
        seen.add(key);
        found.push(way);
        if (found.length >= 48) break;
      }
      const ways = sameCase(prune(found)).slice(0, 24);
      return { id, title: titleOf(id), sort: labels[id] || id, ways };
    })
    .sort((a, b) => a.sort.localeCompare(b.sort));
}
