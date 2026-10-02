export function fillName(text, who = "someone") {
  const name = who || "someone";
  const titled = name.slice(0, 1).toUpperCase() + name.slice(1);
  return String(text ?? "").replace(/\{name\}/g, (_match, offset, all) => {
    const before = all.slice(0, offset);
    return offset === 0 || /[.!?]\s*$/.test(before) ? titled : name;
  });
}

export function slug(title) {
  return title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_|_$/g, "");
}

export function uniqueSlug(title, taken) {
  const base = slug(title);
  if (!base) return "";
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base}_${n}`)) n += 1;
  return `${base}_${n}`;
}

function roleOf(fact) {
  if (fact.role === "fact" || fact.role === "criterion" || fact.role === "theorem") return fact.role;
  return fact.formula ? "theorem" : "fact";
}

export function atomChoices(facts, labels) {
  const atoms = new Map();
  for (const fact of facts) {
    const role = roleOf(fact);
    if (role === "theorem") continue;
    atoms.set(fact.id, { id: fact.id, title: fact.claim, role, hay: fact.claim.toLowerCase() });
  }
  const theorems = new Set(facts.filter((fact) => roleOf(fact) === "theorem").map((fact) => fact.id));
  for (const [id, title] of Object.entries(labels || {})) {
    const name = String(title);
    const have = atoms.get(id);
    if (have) have.hay += ` ${name.toLowerCase()}`;
    else if (!theorems.has(id)) atoms.set(id, { id, title: name, role: "outcome", hay: name.toLowerCase() });
  }
  const rows = [...atoms.values()];
  for (const fact of facts) {
    if (roleOf(fact) !== "theorem" || !fact.formula) continue;
    const claim = readClaim(fact.formula);
    if (!claim) continue;
    const atom = atoms.get(claim.thenId);
    rows.push({
      id: claim.thenId,
      title: fact.claim,
      role: "theorem",
      hay: `${fact.claim} ${atom?.title || ""}`.toLowerCase(),
    });
    if (atom) atom.hay += ` ${fact.claim.toLowerCase()}`;
  }
  return rows.sort((a, b) => a.title.localeCompare(b.title) || a.role.localeCompare(b.role));
}

export function compileClaim(join, partIds, thenId) {
  const left =
    join === "or"
      ? `or(${partIds.map((id) => `fact:${id}`).join(", ")})`
      : partIds.length === 1
        ? `fact:${partIds[0]}`
        : `and(${partIds.map((id) => `fact:${id}`).join(", ")})`;
  return `imp(${left}, fact:${thenId})`;
}

export function readClaim(formula) {
  const match = formula.trim().match(/^imp\((.*), fact:([A-Za-z0-9_]+)\)$/);
  if (!match) return null;
  const [body, thenId] = [match[1], match[2]];
  const or = body.match(/^or\((.*)\)$/);
  const and = body.match(/^and\((.*)\)$/);
  const one = body.match(/^fact:([A-Za-z0-9_]+)$/);
  const ids = (text) =>
    text
      .split(",")
      .map((part) => part.trim().replace(/^fact:/, ""))
      .filter(Boolean);
  if (or) return { join: "or", partIds: ids(or[1]), thenId };
  if (and) return { join: "and", partIds: ids(and[1]), thenId };
  if (one) return { join: "and", partIds: [one[1]], thenId };
  return null;
}
