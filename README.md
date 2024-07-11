# typed-agents

type theory for interaction nets.

run `cargo run test.itt` to try it out.

## Syntax

```
statement = decl | def | check
decl = typed_match ":" (tree ":")* untyped_match
def = untyped_match "~" untyped_match
check = "check" ("yes" | "no") tree "~" tree
untyped_match = agent_name | agent_name "(" (tree)* ")"
typed_match = agent_name | agent_name "(" (tree "->" tree ":" tree)* ")"
tree = agent | var_name | tree_with
agent = agent_name | agent_name "(" (tree)* ")"
tree_with = tree "~" tree "with" tree
agent_name = uppercase_char any_char*
var_name = lowercase_char any_char*
```