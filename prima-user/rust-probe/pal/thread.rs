use crate::time::Duration;
unsafe extern "C" {
    fn __prima_std_sleep(seconds: u64, nanos: u32);
    fn __prima_std_yield();
}
pub fn sleep(duration: Duration) {
    // SAFETY: value-only hooks; native entry initializes the physical clock.
    unsafe { __prima_std_sleep(duration.as_secs(), duration.subsec_nanos()) }
}
pub fn yield_now() {
    // SAFETY: value-only native scheduler hook.
    unsafe { __prima_std_yield() }
}
