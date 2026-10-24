import Lake
open Lake DSL

package formal where
  leanOptions := #[⟨`autoImplicit, false⟩, ⟨`warningAsError, true⟩]

@[default_target]
lean_lib Formal
