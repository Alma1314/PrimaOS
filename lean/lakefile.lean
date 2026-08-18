import Lake
open Lake DSL

package formal where
  require std from git
    "https://github.com/leanprover/std4.git" @ "main"

@[default_target]
lean_lib Formal
