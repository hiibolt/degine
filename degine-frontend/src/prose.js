export function leanSuffix(id) {
  let out = "";
  for (const char of id) {
    out += /[A-Za-z0-9_]/.test(char) ? char : "_";
  }
  if (/[0-9]/.test(out[0] || "")) out = `x${out}`;
  return out;
}

export function matchDecl(name, facts, rules) {
  const factPrefix = "Debate.fact_";
  const rulePrefix = "Debate.rule_";
  if (name.startsWith(factPrefix)) {
    const hits = facts.filter((fact) => leanSuffix(fact.id) === name.slice(factPrefix.length));
    return hits.length === 1
      ? { kind: "fact", id: hits[0].id, item: hits[0] }
      : { kind: "fact", id: null, lean: name };
  }
  if (name.startsWith(rulePrefix)) {
    const hits = rules.filter((rule) => leanSuffix(rule.id) === name.slice(rulePrefix.length));
    return hits.length === 1
      ? { kind: "rule", id: hits[0].id, item: hits[0] }
      : { kind: "rule", id: null, lean: name };
  }
  return { kind: "other", id: null, lean: name };
}

export function shortDiagnostic(text) {
  if (!text) return "";
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  const interesting = lines.filter(
    (line) =>
      line.startsWith("error:") ||
      line.startsWith("⊢") ||
      line.includes("does not follow") ||
      line.startsWith("rule `"),
  );
  return (interesting.length ? interesting : lines.slice(0, 4)).join("\n");
}

function parseFormula(input) {
  let i = 0;
  const source = input.trim();
  const skip = () => {
    while (source[i] && /\s/.test(source[i])) i += 1;
  };
  const parse = () => {
    skip();
    for (const word of ["true", "false"]) {
      if (source.startsWith(word, i) && !/[A-Za-z0-9_]/.test(source[i + word.length] || "")) {
        i += word.length;
        return { kind: word };
      }
    }
    if (source.startsWith("fact:", i)) {
      i += 5;
      let id = "";
      while (i < source.length && source[i] !== "," && source[i] !== ")" && !/\s/.test(source[i])) {
        id += source[i];
        i += 1;
      }
      if (!id) throw new Error("missing fact id");
      return { kind: "atom", id };
    }
    for (const name of ["and", "or", "imp", "not"]) {
      if (!source.startsWith(`${name}(`, i)) continue;
      i += name.length + 1;
      const args = [];
      skip();
      if (source[i] === ")") throw new Error("empty call");
      while (true) {
        args.push(parse());
        skip();
        if (source[i] === ",") {
          i += 1;
          continue;
        }
        if (source[i] === ")") {
          i += 1;
          break;
        }
        throw new Error("expected , or )");
      }
      return { kind: name, args };
    }
    throw new Error("unrecognized formula");
  };
  const formula = parse();
  skip();
  if (i !== source.length) throw new Error("trailing text");
  return formula;
}

function speak(formula, facts, nested) {
  let text;
  if (formula.kind === "true") text = "something already true";
  else if (formula.kind === "false") text = "a contradiction";
  else if (formula.kind === "atom") {
    const fact = facts.find((item) => item.id === formula.id);
    text = fact?.claim ? `${formula.id} ("${fact.claim}")` : formula.id;
  } else if (formula.kind === "not") {
    text = `it is not the case that ${speak(formula.args[0], facts, true)}`;
  } else if (formula.kind === "imp") {
    text = `if ${speak(formula.args[0], facts, true)}, then ${speak(formula.args[1], facts, true)}`;
  } else if (formula.kind === "or") {
    text = `either ${speak(formula.args[0], facts, true)}, or ${speak(formula.args[1], facts, true)}`;
  } else if (formula.kind === "and") {
    text = formula.args.map((arg) => speak(arg, facts, true)).join(", and ");
  } else text = "a formula";
  const compound = !["atom", "true", "false"].includes(formula.kind);
  return nested && compound ? `(${text})` : text;
}

export function readFormula(formula) {
  if (!formula || !formula.trim()) return null;
  try {
    return { ok: true, node: parseFormula(formula) };
  } catch (err) {
    return { ok: false, message: err.message };
  }
}

export function explainFormula(formula, facts) {
  if (!formula) return "";
  try {
    return speak(parseFormula(formula), facts, false);
  } catch {
    return "";
  }
}

function atomsOf(formula) {
  if (!formula) return [];
  try {
    const found = [];
    const walk = (node) => {
      if (node.kind === "atom" && !found.includes(node.id)) found.push(node.id);
      for (const arg of node.args || []) walk(arg);
    };
    walk(parseFormula(formula));
    return found;
  } catch {
    const found = [];
    for (const match of formula.matchAll(/fact:([^,\s)]+)/g)) {
      if (!found.includes(match[1])) found.push(match[1]);
    }
    return found;
  }
}

export function relevantPieces(rule, facts, rules) {
  const ruleIds = new Set();
  const stack = [rule.id];
  while (stack.length) {
    const id = stack.pop();
    if (ruleIds.has(id)) continue;
    const current = rules.find((item) => item.id === id);
    if (!current) continue;
    ruleIds.add(id);
    for (const premise of current.premises) stack.push(premise);
    for (const atom of atomsOf(current.conclusion)) stack.push(atom);
  }
  const factIds = new Set();
  for (const id of ruleIds) {
    const current = rules.find((item) => item.id === id);
    for (const premise of current.premises) {
      if (facts.some((fact) => fact.id === premise)) factIds.add(premise);
    }
    for (const atom of atomsOf(current.conclusion)) factIds.add(atom);
  }
  let grew = true;
  while (grew) {
    grew = false;
    for (const id of factIds) {
      const fact = facts.find((item) => item.id === id);
      for (const atom of atomsOf(fact?.formula)) {
        if (!factIds.has(atom)) {
          factIds.add(atom);
          grew = true;
        }
      }
    }
  }
  const orderedRules = [];
  const seen = new Set();
  const walk = (id) => {
    if (seen.has(id) || !ruleIds.has(id)) return;
    seen.add(id);
    const current = rules.find((item) => item.id === id);
    for (const premise of current.premises) walk(premise);
    orderedRules.push(current);
  };
  for (const id of ruleIds) walk(id);
  const orderedFacts = facts.filter((fact) => factIds.has(fact.id));
  orderedFacts.sort((a, b) => Number(Boolean(a.formula)) - Number(Boolean(b.formula)) || a.id.localeCompare(b.id));
  return { facts: orderedFacts, rules: orderedRules };
}

function labelOf(decl) {
  if (decl.id) return decl.id;
  return decl.lean || "unknown";
}

export function renderArgument({ rule, facts, rules, record, comments }) {
  const pieces = relevantPieces(rule, facts, rules);
  const lines = [];
  const status = record?.status || "pending";
  lines.push(`# Argument: ${rule.id}`);
  lines.push("");
  lines.push(
    "This is a citation-backed argument checked by Lean. Lean only checks whether the conclusion follows from the facts as written. It does not check whether a citation actually supports its claim. That remains a human judgment.",
  );
  lines.push("");
  lines.push("## Verdict");
  if (status === "proved") lines.push("Lean accepted this chain. The conclusion follows.");
  else if (status === "invalid") lines.push("Lean rejected this chain. The conclusion does not follow as written.");
  else lines.push("Lean has not finished checking this chain.");
  const brief = shortDiagnostic(record?.diagnostics);
  if (brief) {
    lines.push("");
    lines.push(brief);
  }
  lines.push("");
  lines.push("## Conclusion");
  const spoken = explainFormula(rule.conclusion, facts);
  lines.push(spoken ? `${spoken}.` : rule.conclusion);
  lines.push("");
  lines.push(`Exact formula: \`${rule.conclusion}\``);
  lines.push("");
  lines.push("## Facts");
  if (!pieces.facts.length) lines.push("No cited facts are attached to this chain.");
  for (const fact of pieces.facts) {
    lines.push("");
    lines.push(`### ${fact.id}`);
    lines.push(fact.claim);
    if (fact.formula) {
      const warrant = explainFormula(fact.formula, facts);
      lines.push("");
      lines.push("This is a cited warrant, not a bare assertion.");
      if (warrant) lines.push(`Read as: ${warrant}.`);
      lines.push(`Exact formula: \`${fact.formula}\``);
    } else {
      lines.push("");
      lines.push("This is taken as given. Lean treats it as true.");
    }
    lines.push("");
    if (fact.citations.length) {
      lines.push("Citations:");
      for (const citation of fact.citations) lines.push(`- ${citation}`);
    } else {
      lines.push("No citations.");
    }
  }
  const knownFacts = new Set(pieces.facts.map((fact) => fact.id));
  const mentioned = [];
  for (const item of [...pieces.facts, ...pieces.rules]) {
    const formula = item.formula || item.conclusion;
    for (const atom of atomsOf(formula)) {
      if (!knownFacts.has(atom) && !mentioned.includes(atom)) mentioned.push(atom);
    }
  }
  if (mentioned.length) {
    lines.push("");
    lines.push(`Mentioned, but not entered as their own facts: ${mentioned.join(", ")}.`);
  }
  lines.push("");
  lines.push("## Rules");
  for (const item of pieces.rules) {
    lines.push("");
    lines.push(`### ${item.id}`);
    if (!item.premises.length) lines.push("Premises: none.");
    else {
      lines.push("Premises:");
      for (const premise of item.premises) {
        const fact = facts.find((entry) => entry.id === premise);
        const earlier = rules.find((entry) => entry.id === premise);
        if (fact) lines.push(`- fact ${premise}: ${fact.claim}`);
        else if (earlier) lines.push(`- rule ${premise}, concluding \`${earlier.conclusion}\``);
        else lines.push(`- ${premise} (not recorded as a fact or a rule)`);
      }
    }
    const conclusion = explainFormula(item.conclusion, facts);
    lines.push(conclusion ? `Concludes: ${conclusion}.` : `Concludes: ${item.conclusion}`);
    lines.push(`Exact formula: \`${item.conclusion}\``);
  }
  lines.push("");
  lines.push("## What Lean linked");
  const graph = record?.graph;
  if (!graph) {
    lines.push("No dependency graph is stored for this rule yet.");
  } else if (!graph.edges.length) {
    lines.push("Lean accepted the rule and found no further dependencies.");
  } else {
    for (const edge of graph.edges) {
      const source = matchDecl(edge.source, facts, rules);
      const target = matchDecl(edge.target, facts, rules);
      lines.push(`- ${labelOf(source)} rests on ${labelOf(target)}`);
    }
  }
  lines.push("");
  lines.push("## Comments");
  if (!comments.length) lines.push("No comments on these facts, rules, or this conclusion.");
  const grouped = new Map();
  for (const comment of comments) {
    const key = `${comment.target_type} ${comment.target_id}`;
    if (!grouped.has(key)) grouped.set(key, []);
    grouped.get(key).push(comment);
  }
  for (const [key, thread] of grouped) {
    lines.push("");
    lines.push(`### ${key}`);
    for (const comment of thread) {
      lines.push(`- ${comment.author} (${comment.created_at}): ${comment.body}`);
    }
  }
  if (record?.diagnostics) {
    lines.push("");
    lines.push("## Full checker output");
    lines.push("");
    lines.push(record.diagnostics.split("\n").map((line) => `    ${line}`).join("\n"));
  }
  lines.push("");
  lines.push("## For an assistant");
  lines.push(
    "You can paste this document to an assistant and ask it to argue the point, pressure-test a citation, or explain the chain in plainer words. The Lean verdict is only about the logic. Citations are claims that a source supports a fact, not proof that it does. Do not invent citations, quotes, or facts that are not written here.",
  );
  lines.push("");
  return lines.join("\n");
}

export function downloadText(filename, text) {
  const blob = new Blob([text], { type: "text/markdown" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}
