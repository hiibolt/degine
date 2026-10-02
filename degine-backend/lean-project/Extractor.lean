import Lean

/-
Dependency extractor.

`#print axioms` only lists root axioms, so it cannot show the chain.
This walks each declaration's type and proof term, records an edge at
every constant, and recurses into that declaration. Stdlib constants
and `Debate.atom_*` proposition symbols are skipped so the graph is
just cited facts and derived rules.
-/

open Lean

namespace Extractor

structure GraphNode where
  name : String
  kind : String
  deriving ToJson

structure GraphEdge where
  source : String
  target : String
  deriving ToJson

structure DepGraph where
  nodes : Array GraphNode
  edges : Array GraphEdge
  deriving ToJson

structure WalkState where
  visited : NameSet := {}
  nodes : Array GraphNode := #[]
  edges : Array GraphEdge := #[]

def kindOf : ConstantInfo → String
  | .axiomInfo _ => "axiom"
  | .thmInfo _ => "theorem"
  | .defnInfo _ => "def"
  | .opaqueInfo _ => "opaque"
  | .quotInfo _ => "quot"
  | .inductInfo _ => "inductive"
  | .ctorInfo _ => "ctor"
  | .recInfo _ => "rec"

/-- `Debate.fact_*` and `Debate.rule_*` are the nodes the app draws. -/
def reportable (n : Name) : Bool :=
  match n with
  | .str (.str .anonymous "Debate") leaf =>
    leaf.startsWith "fact_" || leaf.startsWith "rule_"
  | _ => false

partial def visit (env : Environment) (n : Name) : StateM WalkState Unit := do
  if (← get).visited.contains n then
    return
  modify fun st => { st with visited := st.visited.insert n }
  let some info := env.find? n | return
  if reportable n then
    modify fun st => { st with nodes := st.nodes.push { name := toString n, kind := kindOf info } }
  let exprs : List Expr :=
    match info.value? (allowOpaque := true) with
    | some value => [value, info.type]
    | none => [info.type]
  for expr in exprs do
    -- `getUsedConstants` walks the expr and visits each `.const` once.
    for const in expr.getUsedConstants do
      if reportable const then
        if reportable n && n != const then
          let src := toString n
          let tgt := toString const
          modify fun st =>
            if st.edges.any (fun edge => edge.source == src && edge.target == tgt) then
              st
            else
              { st with edges := st.edges.push { source := src, target := tgt } }
        visit env const

def extract (env : Environment) (target : Name) : Except String DepGraph := do
  if !reportable target then
    throw s!"'{target}' is not a Debate fact or rule"
  let some _ := env.find? target
    | throw s!"unknown declaration {target}"
  let (_, state) := (visit env target).run {}
  return { nodes := state.nodes, edges := state.edges }

end Extractor

def main (args : List String) : IO UInt32 := do
  let some arg := args[0]? | do
    IO.eprintln "usage: extractor <Debate.rule_...>"
    return 1
  initSearchPath (← findSysroot)
  let env ← importModules #[{ module := `Debate.Generated : Import }] {}
  match Extractor.extract env arg.toName with
  | .ok graph =>
    IO.println (toJson graph).compress
    return 0
  | .error err =>
    IO.eprintln err
    return 1
