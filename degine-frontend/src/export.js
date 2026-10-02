import { readClaim } from "./phrases.js";
import { explainFormula, shortDiagnostic } from "./prose.js";

function atomsOf(formula) {
  const found = [];
  for (const match of String(formula || "").matchAll(/fact:([^,\s)]+)/g)) {
    if (!found.includes(match[1])) found.push(match[1]);
  }
  return found;
}

function roleOf(fact) {
  if (fact.role === "fact" || fact.role === "criterion" || fact.role === "theorem") return fact.role;
  return fact.formula ? "theorem" : "fact";
}

export function argumentPieces(assert, facts) {
  const atoms = new Set(atomsOf(assert.formula));
  const used = [];
  let grew = true;
  while (grew) {
    grew = false;
    for (const fact of facts) {
      if (used.some((item) => item.id === fact.id)) continue;
      if (roleOf(fact) !== "theorem") {
        if (atoms.has(fact.id)) {
          used.push(fact);
          grew = true;
        }
        continue;
      }
      const claim = readClaim(fact.formula || "");
      if (!claim || !atoms.has(claim.thenId)) continue;
      used.push(fact);
      for (const id of [claim.thenId, ...claim.partIds, ...atomsOf(fact.formula)]) atoms.add(id);
      grew = true;
    }
  }
  return used;
}

function say(formula, titleOf) {
  const claim = readClaim(formula || "");
  if (!claim) return explainFormula(formula, []) || formula;
  const left = claim.partIds.map((id) => titleOf(id)).join(claim.join === "or" ? " or " : " and ");
  return `if ${left}, then ${titleOf(claim.thenId)}`;
}

export function renderAssert({ assert, facts, labels, record, comments }) {
  const titleOf = (id) => facts.find((fact) => fact.id === id)?.claim || labels[id] || id;
  const pieces = argumentPieces(assert, facts);
  const status = record?.status || "pending";
  const lines = [
    `# ${assert.title}`,
    "",
    assert.description || "",
    "",
    "This is a citation-backed argument checked by Lean. Lean only checks whether the conclusion follows from the formulas as written. It does not check whether a citation actually supports its claim.",
    "",
    "## Verdict",
  ];
  if (status === "proved") lines.push("Lean accepted this argument. The conclusion follows.");
  else if (status === "invalid") lines.push("Lean rejected this argument. The conclusion does not follow as written.");
  else lines.push("Lean has not finished checking this argument.");
  const brief = shortDiagnostic(record?.diagnostics);
  if (brief) {
    lines.push("");
    lines.push(brief);
  }
  lines.push("", "## Claim", `${say(assert.formula, titleOf)}.`, "", `Exact formula: \`${assert.formula}\``);
  const factsOnly = pieces.filter((fact) => roleOf(fact) === "fact");
  const criteria = pieces.filter((fact) => roleOf(fact) === "criterion");
  const theorems = pieces.filter((fact) => roleOf(fact) === "theorem");
  lines.push("", "## Facts");
  if (!factsOnly.length) lines.push("None in this argument.");
  for (const fact of factsOnly) {
    lines.push("", `### ${fact.claim}`);
    lines.push("Taken as given.");
    if (fact.citations?.length) {
      lines.push("", "Citations:");
      for (const citation of fact.citations) lines.push(`- ${citation}`);
    } else lines.push("", "No citations.");
  }
  lines.push("", "## Criteria");
  if (!criteria.length) lines.push("None in this argument.");
  for (const fact of criteria) lines.push("", `### ${fact.claim}`, "A condition, not claimed as true.");
  lines.push("", "## Theorems");
  if (!theorems.length) lines.push("None in this argument.");
  for (const fact of theorems) {
    lines.push("", `### ${fact.claim}`, say(fact.formula, titleOf));
    if (fact.citations?.length) {
      lines.push("", "Citations:");
      for (const citation of fact.citations) lines.push(`- ${citation}`);
    }
  }
  lines.push("", "## Comments");
  if (!comments.length) lines.push("No comments.");
  for (const comment of comments) {
    const where = comment.target_type === "assert" ? assert.title : titleOf(comment.target_id);
    const mark = comment.resolved ? "resolved" : "open";
    lines.push(`- ${where} · ${comment.author} · ${mark}: ${comment.body}`);
  }
  lines.push(
    "",
    "## For an assistant",
    "You can paste this document to an assistant and ask it to argue the point, pressure-test a citation, or explain the chain in plainer words. The Lean verdict is only about the logic. Do not invent citations, quotes, or facts that are not written here.",
    "",
  );
  return lines.join("\n");
}
