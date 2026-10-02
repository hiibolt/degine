<script>
  let {
    facts,
    rules,
    graphs,
    comments,
    selected,
    drafting,
    busy,
    matchDecl,
    shortDiagnostic,
    onsaveFact,
    onsaveRule,
    onremove,
    oncomment,
    onexport,
    onchoose,
    onerror,
  } = $props();

  let factId = $state("");
  let claim = $state("");
  let citations = $state("");
  let formula = $state("");
  let ruleId = $state("");
  let premises = $state("");
  let conclusion = $state("");
  let commentBody = $state("");
  let thread = $state("conclusion");
  let seen = "";

  function lines(value) {
    return value
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  }

  function blank(value) {
    const text = value.trim();
    return text ? text : null;
  }

  $effect(() => {
    const key = drafting ? `new:${drafting}` : selected ? `${selected.kind}:${selected.id}` : "";
    if (key === seen) return;
    seen = key;
    commentBody = "";
    thread = "conclusion";
    if (drafting === "fact" || selected?.kind === "fact") {
      const fact = drafting ? null : facts.find((item) => item.id === selected.id);
      factId = fact?.id || "";
      claim = fact?.claim || "";
      citations = fact?.citations.join("\n") || "";
      formula = fact?.formula || "";
    }
    if (drafting === "rule" || selected?.kind === "rule") {
      const rule = drafting ? null : rules.find((item) => item.id === selected.id);
      ruleId = rule?.id || "";
      premises = rule?.premises.join("\n") || "";
      conclusion = rule?.conclusion || "";
    }
  });

  const openFact = $derived(
    !drafting && selected?.kind === "fact" ? facts.find((item) => item.id === selected.id) : null,
  );
  const openRule = $derived(
    !drafting && selected?.kind === "rule" ? rules.find((item) => item.id === selected.id) : null,
  );
  const record = $derived(openRule ? graphs[openRule.id] : null);
  const shownFacts = $derived(openFact ? comments[`fact:${openFact.id}`] || [] : []);
  const shownRules = $derived(openRule ? comments[`rule:${openRule.id}`] || [] : []);
  const shownConclusions = $derived(openRule ? comments[`conclusion:${openRule.id}`] || [] : []);

  async function submitFact(event) {
    event.preventDefault();
    const body = {
      claim,
      citations: lines(citations),
      formula: blank(formula),
    };
    if (!body.citations.length && !confirm("save this fact with no citation?")) {
      return;
    }
    try {
      await onsaveFact(drafting ? { id: factId.trim(), ...body } : body, openFact?.id);
    } catch (err) {
      onerror(err);
    }
  }

  async function submitRule(event) {
    event.preventDefault();
    const body = { premises: lines(premises), conclusion: conclusion.trim() };
    try {
      await onsaveRule(drafting ? { id: ruleId.trim(), ...body } : body, openRule?.id);
    } catch (err) {
      onerror(err);
    }
  }

  async function submitComment(event) {
    event.preventDefault();
    const targetType = openFact ? "fact" : thread;
    const targetId = openFact?.id || openRule?.id;
    try {
      await oncomment(targetType, targetId, commentBody.trim());
      commentBody = "";
    } catch (err) {
      onerror(err);
    }
  }

  async function remove() {
    if (!confirm("delete this?")) return;
    try {
      await onremove();
    } catch (err) {
      onerror(err);
    }
  }
</script>

<section class="card stack">
  {#if !drafting && !selected}
    <h2>nothing open</h2>
    <p class="muted">Pick a fact or a rule, or add one. A rule is the conclusion Lean checks.</p>
  {:else if drafting === "fact" || selected?.kind === "fact"}
    <div class="spread row">
      <h2>{openFact ? openFact.id : "new fact"}</h2>
      {#if openFact}<button class="quiet" type="button" onclick={remove}>delete</button>{/if}
    </div>
    <form class="stack" onsubmit={submitFact}>
      {#if !openFact}
        <label>id <input bind:value={factId} /></label>
      {/if}
      <label>claim <textarea bind:value={claim}></textarea></label>
      <label>
        citations, one per line
        <textarea bind:value={citations}></textarea>
      </label>
      <p class="hint">A message, an article, a document. Use as many as the claim needs. Saving with none asks you to confirm.</p>
      <label>formula <input bind:value={formula} /></label>
      <p class="hint">Leave this empty to assert the fact. A warrant looks like imp(fact:signed, fact:binding).</p>
      <button class="primary" type="submit">save</button>
    </form>
  {:else}
    <div class="spread row">
      <h2>{openRule ? openRule.id : "new rule"}</h2>
      <div class="actions">
        {#if openRule}
          <button type="button" disabled={busy} onclick={() => onexport(openRule)}>
            {busy ? "preparing..." : "export for an assistant"}
          </button>
          <button class="quiet" type="button" onclick={remove}>delete</button>
        {/if}
      </div>
    </div>
    {#if openRule}
      {@const status = record?.status || "pending"}
      <p class="verdict {status}">
        {#if status === "proved"}
          Lean accepted this chain.
        {:else if status === "invalid"}
          Lean rejected this chain.
        {:else}
          Lean is still checking this chain.
        {/if}
      </p>
      {#if record?.diagnostics}
        <pre class="diagnostic">{shortDiagnostic(record.diagnostics)}</pre>
      {/if}
      {#if record?.graph}
        <h3>chain</h3>
        <ul class="chain">
          {#each record.graph.nodes as node (node.name)}
            {@const decl = matchDecl(node.name, facts, rules)}
            <li>
              {#if decl.id}
                <button class="quiet" type="button" onclick={() => onchoose(decl.kind, decl.id)}>{decl.id}</button>
                {#if decl.item?.claim}<span class="claim">{decl.item.claim}</span>{/if}
              {:else}
                {node.name}
              {/if}
            </li>
          {/each}
        </ul>
        {#if record.graph.edges.length}
          <h3>rests on</h3>
          <ul class="chain">
            {#each record.graph.edges as edge (`${edge.source}-${edge.target}`)}
              <li>
                {matchDecl(edge.source, facts, rules).id || edge.source}
                rests on
                {matchDecl(edge.target, facts, rules).id || edge.target}
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    {/if}
    <form class="stack" onsubmit={submitRule}>
      {#if !openRule}
        <label>id <input bind:value={ruleId} /></label>
      {/if}
      <label>
        premises, one id per line
        <textarea bind:value={premises}></textarea>
      </label>
      <label>conclusion <input bind:value={conclusion} /></label>
      <p class="hint">The formula this rule has to prove, such as fact:binding.</p>
      <button class="primary" type="submit">save</button>
    </form>
  {/if}

  {#if openFact || openRule}
    <h3>comments</h3>
    <ul class="thread">
      {#each (openFact ? shownFacts : [...shownRules, ...shownConclusions]) as comment (comment.id)}
        <li>
          <strong>{comment.author}</strong>
          <span class="muted">
            {#if openRule}on {comment.target_type} · {/if}
            {comment.created_at.replace("T", " ").slice(0, 16)}
          </span>
          <div>{comment.body}</div>
        </li>
      {:else}
        <li class="muted">no comments yet</li>
      {/each}
    </ul>
    <form class="stack" onsubmit={submitComment}>
      {#if openRule}
        <label>
          comment on
          <select bind:value={thread}>
            <option value="conclusion">this conclusion</option>
            <option value="rule">this rule</option>
          </select>
        </label>
      {/if}
      <label>
        comment
        <textarea bind:value={commentBody}></textarea>
      </label>
      <button type="submit">post</button>
    </form>
  {/if}
</section>
