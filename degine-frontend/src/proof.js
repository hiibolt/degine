import { readClaim } from "./phrases.js";
import { leanSuffix } from "./prose.js";

function roleOf(fact) {
  if (fact.role === "fact" || fact.role === "criterion" || fact.role === "theorem") return fact.role;
  return fact.formula ? "theorem" : "fact";
}

function leaf(id, titleOf, role, cycle) {
  const library = role === "fact" || role === "criterion";
  return { id, title: titleOf(id), role, library, cycle, steps: [], assumed: [] };
}

function explainAtom(id, facts, titleOf, trail, assumed, used) {
  const fact = facts.find((item) => item.id === id);
  const stored = fact ? roleOf(fact) : "";
  const given = stored === "fact" || stored === "criterion" ? stored : "step";
  if (assumed.has(id)) return leaf(id, titleOf, given === "step" ? "assumed" : given, false);
  if (trail.has(id)) return leaf(id, titleOf, "loop", true);
  const next = new Set(trail);
  next.add(id);
  const steps = [];
  for (const item of facts) {
    if (roleOf(item) !== "theorem" || !item.formula) continue;
    if (used && !used.has(item.id)) continue;
    const claim = readClaim(item.formula);
    if (!claim || claim.thenId !== id) continue;
    steps.push({
      theoremId: item.id,
      theoremTitle: item.claim,
      join: claim.join,
      needs: claim.partIds.map((part) => explainAtom(part, facts, titleOf, next, assumed, used)),
    });
  }
  return {
    id,
    title: titleOf(id),
    role: given,
    library: given !== "step",
    cycle: false,
    steps,
    assumed: [],
  };
}

function usedTheorems(graph, facts) {
  if (!graph?.edges?.length) return null;
  const prefix = "Debate.fact_";
  const ids = new Set();
  for (const edge of graph.edges) {
    const name = edge.target || "";
    if (!name.startsWith(prefix)) continue;
    const suffix = name.slice(prefix.length);
    const hit = facts.find((fact) => roleOf(fact) === "theorem" && leanSuffix(fact.id) === suffix);
    if (hit) ids.add(hit.id);
  }
  return ids;
}

function collect(node, seen) {
  if (seen.has(node.id)) return;
  seen.add(node.id);
  for (const step of node.steps) {
    for (const need of step.needs) collect(need, seen);
  }
}

export function explainItem(item, facts, titleOf) {
  const role = roleOf(item);
  if (role === "theorem" && item.formula) {
    const claim = readClaim(item.formula);
    if (!claim) return null;
    const trail = new Set([item.id, claim.thenId]);
    return {
      id: item.id,
      title: item.claim,
      role: "theorem",
      library: false,
      cycle: false,
      steps: [
        {
          theoremId: item.id,
          theoremTitle: "",
          join: claim.join,
          needs: claim.partIds.map((part) => explainAtom(part, facts, titleOf, trail, new Set(), null)),
        },
      ],
      assumed: [],
    };
  }
  return { ...leaf(item.id, () => item.claim, role === "criterion" ? "criterion" : "fact", false), library: false };
}

export function explainAssert(formula, facts, titleOf, graph) {
  const claim = readClaim(formula);
  if (!claim) return null;
  const assumed = new Set(claim.partIds);
  const used = usedTheorems(graph, facts);
  const conclusion = explainAtom(claim.thenId, facts, titleOf, new Set(), assumed, used);
  const seen = new Set();
  collect(conclusion, seen);
  conclusion.assumed = claim.partIds
    .filter((id) => !seen.has(id))
    .map((id) => explainAtom(id, facts, titleOf, new Set(), new Set(), null));
  return conclusion;
}
