import Lake
open Lake DSL

require formal from ".."
require aeneas from "../../target/aeneas/backends/lean"

package kernel_refinement where
  leanOptions := #[⟨`autoImplicit, false⟩, ⟨`warningAsError, true⟩]

@[default_target]
lean_lib Refinement
