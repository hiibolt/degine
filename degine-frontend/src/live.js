// Library reads and presence. Writes stay on the backend.

const PAGE = 1000;
const CAP = 5000;

function db(supabase) {
  return supabase.schema("degine");
}

async function rows(query) {
  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data || [];
}

async function allRows(build) {
  const out = [];
  for (let from = 0; from < CAP; from += PAGE) {
    const page = await rows(build().range(from, from + PAGE - 1));
    out.push(...page);
    if (page.length < PAGE) break;
  }
  return out;
}

export function emptyBag() {
  return {
    facts: [],
    citations: [],
    rules: [],
    premises: [],
    labels: [],
    asserts: [],
    comments: [],
    graphs: [],
    assertGraphs: [],
    members: [],
    profiles: [],
    workspace: null,
  };
}

export async function listWorkspaces(supabase, userId) {
  const memberships = await allRows(() =>
    db(supabase).from("workspace_members").select("workspace_id, role").eq("user_id", userId),
  );
  const ids = memberships.map((item) => item.workspace_id);
  if (!ids.length) return [];
  const spaces = await rows(
    db(supabase).from("workspaces").select("id, name, created_by").in("id", ids).limit(CAP),
  );
  const role = Object.fromEntries(memberships.map((item) => [item.workspace_id, item.role]));
  return spaces
    .map((space) => ({
      id: space.id,
      name: space.name,
      role: role[space.id] || "reader",
      creator: space.created_by === userId,
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id));
}

export async function loadLibrary(supabase, workspaceId) {
  const scoped = (table, cols) => allRows(() => db(supabase).from(table).select(cols).eq("workspace_id", workspaceId));
  const [facts, citations, rules, premises, labels, asserts, comments, graphs, assertGraphs, members, spaces] =
    await Promise.all([
      scoped("facts", "id, claim, formula, role"),
      scoped("fact_citations", "fact_id, position, reference"),
      scoped("rules", "id, conclusion"),
      scoped("rule_premises", "rule_id, position, premise_id"),
      scoped("labels", "id, title"),
      scoped("asserts", "id, title, description, formula"),
      scoped("comments", "id, target_type, target_id, author, body, created_at, resolved"),
      scoped("graphs", "rule_id, status, graph_json, diagnostics"),
      scoped("assert_graphs", "assert_id, status, graph_json, diagnostics"),
      scoped("workspace_members", "user_id, role"),
      rows(db(supabase).from("workspaces").select("id, name, created_by").eq("id", workspaceId).limit(1)),
    ]);
  const ids = [...new Set(members.map((item) => item.user_id))];
  const profiles = ids.length
    ? await rows(db(supabase).from("profiles").select("user_id, username").in("user_id", ids).limit(CAP))
    : [];
  return {
    facts,
    citations,
    rules,
    premises,
    labels,
    asserts,
    comments,
    graphs,
    assertGraphs,
    members,
    profiles,
    workspace: spaces[0] || null,
  };
}

export async function profileNames(supabase, ids) {
  if (!ids.length) return [];
  return rows(db(supabase).from("profiles").select("user_id, username").in("user_id", ids).limit(CAP));
}

function byId(list, item) {
  const next = list.filter((entry) => entry.id !== item.id);
  next.push(item);
  return next;
}

export function putFact(bag, fact) {
  bag.facts = byId(bag.facts, {
    id: fact.id,
    claim: fact.claim,
    formula: fact.formula ?? null,
    role: fact.role,
  });
  bag.citations = bag.citations.filter((item) => item.fact_id !== fact.id);
  (fact.citations || []).forEach((reference, position) => {
    bag.citations.push({ fact_id: fact.id, position, reference });
  });
}

export function dropFact(bag, id) {
  bag.facts = bag.facts.filter((item) => item.id !== id);
  bag.citations = bag.citations.filter((item) => item.fact_id !== id);
}

export function putRule(bag, rule) {
  bag.rules = byId(bag.rules, { id: rule.id, conclusion: rule.conclusion });
  bag.premises = bag.premises.filter((item) => item.rule_id !== rule.id);
  (rule.premises || []).forEach((premise_id, position) => {
    bag.premises.push({ rule_id: rule.id, position, premise_id });
  });
}

export function dropRule(bag, id) {
  bag.rules = bag.rules.filter((item) => item.id !== id);
  bag.premises = bag.premises.filter((item) => item.rule_id !== id);
  bag.graphs = bag.graphs.filter((item) => item.rule_id !== id);
}

export function putAssert(bag, item) {
  bag.asserts = byId(bag.asserts, {
    id: item.id,
    title: item.title,
    description: item.description,
    formula: item.formula,
  });
  const prefix = `~assume:${item.id}:`;
  bag.labels = bag.labels.filter((label) => !label.id.startsWith(prefix));
  for (const id of item.assumes || []) bag.labels.push({ id: `${prefix}${id}`, title: "on" });
}

export function dropAssert(bag, id) {
  bag.asserts = bag.asserts.filter((item) => item.id !== id);
  bag.assertGraphs = bag.assertGraphs.filter((item) => item.assert_id !== id);
  const prefix = `~assume:${id}:`;
  bag.labels = bag.labels.filter((label) => !label.id.startsWith(prefix));
}

export function putLabel(bag, id, title) {
  const found = bag.labels.find((label) => label.id === id);
  if (found) found.title = title;
  else bag.labels.push({ id, title });
}

export function dropLabel(bag, id) {
  bag.labels = bag.labels.filter((label) => label.id !== id);
}

export function dropLabelWhere(bag, pred) {
  bag.labels = bag.labels.filter((label) => !pred(label.id));
}

export function putComment(bag, comment) {
  const id = Number(comment.id);
  const row = { ...comment, id };
  const index = bag.comments.findIndex((item) => Number(item.id) === id);
  if (index >= 0) bag.comments[index] = row;
  else bag.comments.push(row);
}

export function dropComment(bag, id) {
  bag.comments = bag.comments.filter((item) => Number(item.id) !== Number(id));
}

export function markPending(bag, kind, id) {
  const list = kind === "assert" ? bag.assertGraphs : bag.graphs;
  const key = kind === "assert" ? "assert_id" : "rule_id";
  const found = list.find((item) => item[key] === id);
  if (found) {
    found.status = "pending";
    found.graph_json = null;
    found.diagnostics = null;
  } else {
    list.push({ [key]: id, status: "pending", graph_json: null, diagnostics: null });
  }
}

const tableKeys = {
  facts: ["id"],
  fact_citations: ["fact_id", "position"],
  rules: ["id"],
  rule_premises: ["rule_id", "position"],
  labels: ["id"],
  asserts: ["id"],
  comments: ["id"],
  graphs: ["rule_id"],
  assert_graphs: ["assert_id"],
  workspace_members: ["user_id"],
};

const bagList = {
  fact_citations: "citations",
  rule_premises: "premises",
  assert_graphs: "assertGraphs",
  workspace_members: "members",
};

function same(a, b, fields) {
  return fields.every((field) => String(a?.[field]) === String(b?.[field]));
}

export function applyRealtime(bag, table, payload) {
  const type = payload.eventType || payload.event_type;
  if (table === "workspaces") {
    if (type === "DELETE") bag.workspace = null;
    else if (payload.new) bag.workspace = payload.new;
    return;
  }
  const field = tableKeys[table];
  const listName = bagList[table] || table;
  if (!field || !bag[listName]) return;
  if (type === "DELETE") {
    const gone = payload.old || {};
    bag[listName] = bag[listName].filter((item) => !same(item, gone, field));
    return;
  }
  const row = payload.new;
  if (!row) return;
  const index = bag[listName].findIndex((item) => same(item, row, field));
  if (index >= 0) bag[listName][index] = row;
  else bag[listName].push(row);
}

function graphOf(row) {
  let graph = null;
  if (row.graph_json) {
    try {
      graph = JSON.parse(row.graph_json);
    } catch {
      graph = null;
    }
  }
  return { status: row.status || "pending", graph, diagnostics: row.diagnostics || null };
}

function grouped(rows, fields) {
  const map = new Map();
  const sorted = [...rows].sort((a, b) => Number(a.position) - Number(b.position));
  for (const row of sorted) {
    const key = fields.map((field) => row[field]).join("\0");
    const list = map.get(key) || [];
    list.push(row);
    map.set(key, list);
  }
  return map;
}

export function project(bag, userId) {
  const citations = grouped(bag.citations, ["fact_id"]);
  const premises = grouped(bag.premises, ["rule_id"]);
  const facts = [...bag.facts]
    .map((fact) => ({
      id: fact.id,
      claim: fact.claim,
      formula: fact.formula || null,
      role: fact.role,
      citations: (citations.get(fact.id) || []).map((item) => item.reference),
    }))
    .sort((a, b) => a.id.localeCompare(b.id));
  const rules = [...bag.rules]
    .map((rule) => ({
      id: rule.id,
      conclusion: rule.conclusion,
      premises: (premises.get(rule.id) || []).map((item) => item.premise_id),
    }))
    .sort((a, b) => a.id.localeCompare(b.id));
  const assumes = new Map();
  const labels = {};
  const people = [];
  const personal = [];
  const toggles = new Map();
  for (const label of bag.labels) {
    if (label.id.startsWith("~assume:")) {
      const rest = label.id.slice("~assume:".length);
      const split = rest.indexOf(":");
      if (split < 0) continue;
      const assertId = rest.slice(0, split);
      const factId = rest.slice(split + 1);
      const list = assumes.get(assertId) || [];
      list.push(factId);
      assumes.set(assertId, list);
    } else if (label.id.startsWith("~person:")) {
      people.push({ id: label.id.slice("~person:".length), name: label.title, on: [] });
    } else if (label.id.startsWith("~pfact:")) {
      personal.push({ id: label.id.slice("~pfact:".length), claim: label.title });
    } else if (label.id.startsWith("~on:")) {
      const rest = label.id.slice("~on:".length);
      const split = rest.indexOf(":");
      if (split < 0) continue;
      const person = rest.slice(0, split);
      const fact = rest.slice(split + 1);
      const list = toggles.get(person) || [];
      list.push(fact);
      toggles.set(person, list);
    } else {
      labels[label.id] = label.title;
    }
  }
  people.sort((a, b) => a.name.localeCompare(b.name));
  for (const person of people) person.on = toggles.get(person.id) || [];
  personal.sort((a, b) => a.claim.localeCompare(b.claim));
  const asserts = [...bag.asserts]
    .map((item) => ({
      id: item.id,
      title: item.title,
      description: item.description,
      formula: item.formula,
      assumes: assumes.get(item.id) || [],
    }))
    .sort((a, b) => a.id.localeCompare(b.id));
  const comments = {};
  const ordered = [...bag.comments].sort((a, b) => Number(a.id) - Number(b.id));
  for (const comment of ordered) {
    const row = {
      id: Number(comment.id),
      target_type: comment.target_type,
      target_id: comment.target_id,
      author: comment.author,
      body: comment.body,
      created_at: comment.created_at,
      resolved: Boolean(comment.resolved),
    };
    const key = `${row.target_type}:${row.target_id}`;
    (comments[key] ||= []).push(row);
  }
  const graphs = {};
  for (const row of bag.graphs) graphs[row.rule_id] = graphOf(row);
  for (const row of bag.assertGraphs) graphs[row.assert_id] = graphOf(row);
  const names = Object.fromEntries(bag.profiles.map((item) => [item.user_id, item.username]));
  const createdBy = bag.workspace?.created_by || "";
  const members = [...bag.members]
    .map((item) => ({
      user_id: item.user_id,
      username: names[item.user_id] || null,
      role: item.role,
      creator: item.user_id === createdBy,
    }))
    .sort((a, b) => (a.username || "\uffff").localeCompare(b.username || "\uffff") || a.user_id.localeCompare(b.user_id));
  const mine = bag.members.find((item) => item.user_id === userId);
  return {
    facts,
    rules,
    asserts,
    labels,
    graphs,
    people,
    personal,
    comments,
    members,
    workspace: bag.workspace,
    role: mine?.role || "reader",
  };
}

const tables = [
  "facts",
  "fact_citations",
  "rules",
  "rule_premises",
  "labels",
  "asserts",
  "comments",
  "graphs",
  "assert_graphs",
  "workspace_members",
];

export function watchLibrary(supabase, workspaceId, userId, onEvent, onSelf, onStatus) {
  let seen = false;
  const channel = supabase.channel(`library:${workspaceId}`, { config: { private: true } });
  for (const table of tables) {
    channel.on(
      "postgres_changes",
      { event: "*", schema: "degine", table, filter: `workspace_id=eq.${workspaceId}` },
      (payload) => onEvent(table, payload),
    );
  }
  channel.on(
    "postgres_changes",
    { event: "*", schema: "degine", table: "workspaces", filter: `id=eq.${workspaceId}` },
    (payload) => onEvent("workspaces", payload),
  );
  // one binding per table. a second workspace_members filter makes the server
  // reply with a different binding list, and the client then closes the channel.
  channel.subscribe((status, err) => {
    if (status === "SUBSCRIBED") {
      onStatus("open");
      if (seen) onSelf();
      seen = true;
    } else if (status === "CHANNEL_ERROR" || status === "TIMED_OUT" || status === "CLOSED") {
      if (err) console.warn("library channel", status, err.message || err);
      onStatus("closed");
    } else if (status === "CHANNEL_CONNECTING") {
      onStatus("connecting");
    }
  });
  return () => {
    supabase.removeChannel(channel);
  };
}

export function watchPresence(supabase, workspaceId, userId, getBody, onSync) {
  const room = supabase.channel(`workspace:${workspaceId}`, {
    config: { private: true, presence: { key: userId }, broadcast: { self: false } },
  });
  const cursors = {};
  let ready = false;
  let pending = null;
  const push = () => onSync(room.presenceState(), cursors);
  room.on("presence", { event: "sync" }, push);
  room.on("broadcast", { event: "cursor" }, ({ payload }) => {
    if (!payload?.user_id || payload.user_id === userId) return;
    cursors[payload.user_id] = payload;
    push();
  });
  room.subscribe((status) => {
    if (status !== "SUBSCRIBED") return;
    ready = true;
    // one presence update so a late join sees who is here. moves go out as broadcast.
    room.track({ ...getBody(), at: Date.now() });
    if (pending) room.send({ type: "broadcast", event: "cursor", payload: pending });
    pending = null;
  });
  return {
    move(body) {
      if (!ready) {
        pending = body;
        return;
      }
      return room.send({ type: "broadcast", event: "cursor", payload: body });
    },
    stop() {
      supabase.removeChannel(room);
    },
  };
}

export function facesFrom(state, cursors, me, members) {
  if (!state) return [];
  const allowed = new Set(members.map((item) => item.user_id));
  const names = Object.fromEntries(members.map((item) => [item.user_id, item.username]));
  const faces = [];
  for (const [key, metas] of Object.entries(state)) {
    const meta = metas.reduce(
      (best, item) => ((item.at || 0) >= (best.at || 0) ? item : best),
      metas[0] || {},
    );
    const cursor = cursors?.[meta.user_id || key];
    const spot = cursor && (cursor.at || 0) >= (meta.at || 0) ? { ...meta, ...cursor } : meta;
    const id = spot.user_id || key;
    if (!id || id === me || !allowed.has(id)) continue;
    const name = names[id] || "someone";
    const tab = spot.tab || "here";
    const item = spot.item || "";
    let label = tab;
    if (item) {
      const title = String(spot.title || item);
      label = `${tab} · ${title.length > 42 ? `${title.slice(0, 41)}…` : title}`;
    }
    let hue = 0;
    for (const ch of id) hue = (hue * 33 + ch.charCodeAt(0)) % 360;
    const letters = (name.replace(/[^a-z0-9]/gi, "").slice(0, 2) || "?").toLowerCase();
    faces.push({
      user_id: id,
      initials: letters,
      label: `${name} · ${label}`,
      color: `hsl(${hue} 42% 42%)`,
      tab,
      item,
    });
  }
  faces.sort((a, b) => a.label.localeCompare(b.label));
  return faces;
}
