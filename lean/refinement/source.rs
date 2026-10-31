#![no_std]

// The extractor compiles the production module directly, without a copied implementation.
#[path = "../../kernel/src/handle.rs"]
pub mod handle;

pub fn make_handle(index: u32, generation: u32) -> Result<handle::Handle, handle::HandleError> {
    handle::Handle::new(index, generation)
}

pub fn next_generation(value: handle::Handle) -> Option<u32> {
    value.next_generation()
}
