pub const FRAME_SIZE: u64 = 4096;

#[cfg(test)]
mod tests {
    use super::FRAME_SIZE;

    #[test]
    fn frame_size_is_4kib() {
        assert_eq!(FRAME_SIZE, 4096);
    }
}
