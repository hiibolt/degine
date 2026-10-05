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

function product(lists) {
  let rows = [[]];
  for (const list of lists) {
    const next = [];
    for (const row of rows) {
      for (const item of list) {
        next.push([...row, item]);
        if (next.length >= 24) return next;
      }
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
  if (stack.has(id) || stack.size > 12) return [];
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
  return found;
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

let cachedKey = "";
let cachedBoard = [];

function boardKey(facts, labels) {
  const lines = facts.map((fact) => `${fact.id}|${fact.role}|${fact.formula || ""}|${fact.claim}`);
  lines.sort();
  const names = Object.keys(labels || {}).sort();
  for (const id of names) lines.push(`${id}=${labels[id]}`);
  return lines.join("\n");
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
        found.push({ ...way, key, shown: way.assumes.length ? way.assumes : way.givens });
        if (found.length >= 48) break;
      }
      const ways = found
        .filter(
          (way) =>
            !found.some(
              (other) =>
                other.assumes.length < way.assumes.length &&
                other.assumes.every((id) => way.assumes.includes(id)),
            ),
        )
        .slice(0, 24);
      return { id, title: titleOf(id), sort: labels[id] || id, ways };
    })
    .sort((a, b) => a.sort.localeCompare(b.sort));
}
