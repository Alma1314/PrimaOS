//! Checked geometry for the user-space C startup stack.

/// Keep the historical 256-byte minimum for short startup blocks. Larger
/// argv/envp blocks grow down within the existing stack, without truncation.
pub fn stack_offset(stack_bytes: u64, args: &[u8], env: &[u8], aux_words: usize) -> Option<u64> {
    let words = 3usize
        .checked_add(args.len())?
        .checked_add(env.len())?
        .checked_add(aux_words)?;
    let header = (words.checked_mul(8)?.checked_add(15)?) & !15;
    let strings = args
        .iter()
        .chain(env)
        .try_fold(0usize, |size, len| size.checked_add(usize::from(*len) + 1))?;
    let bytes = (header.checked_add(strings)?.checked_add(15)?) & !15;
    stack_bytes
        .checked_sub(bytes.max(256) as u64)
        .filter(|offset| *offset >= 4096 && offset & 15 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_blocks_keep_alignment_and_fit_without_truncation() {
        assert_eq!(
            stack_offset(32768, &[12, 11], &[11, 11, 9], 16),
            Some(32512)
        );
        let args = [255; 16];
        let env = [255; 8];
        let offset = stack_offset(32768, &args, &env, 16).unwrap();
        assert_eq!(offset & 15, 0);
        assert!(32768 - offset >= 344 + 24 * 256);
        assert!(offset >= 4096);
        assert_eq!(stack_offset(4096, &args, &env, 16), None);
        assert_eq!(stack_offset(32768, &[], &[], usize::MAX), None);
    }
}
