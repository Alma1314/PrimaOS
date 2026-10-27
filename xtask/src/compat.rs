//! 切片 P：Linux trace 有界差分（`COMPAT-TEST (bounded: cow-write-fault)`）。
//!
//! 两侧各自运行**同一场景契约**（一个 present、只读、用户可访问的页被用户态写入
//! → 写保护 #PF（P|W|US）→ 建立私有副本 → 重试写成功 → 原副本不变、页内其余内容
//! 保持不变），把可观察语义归一化为 [`CowObservation`] 后比较。
//!
//! 比较是**语义化**的：只比较 error code 位、分类名、页内偏移、解析结果与事件偏序；
//! 不比较绝对地址、pid/tgid、时间戳或整段日志字节（`Prima OS.md` §13.4 要求允许
//! 规范本身允许的非确定性）。
//!
//! 边界：本模块是**有界**差分，不是白皮书 §8.3/§13.4 的完整 `COMPAT-TEST`
//! （后者要求 ELF 加载器、Linux 初始栈与 Prima LibOS 上运行同一二进制）。
//! 两侧也非同一二进制，而是同一场景契约的两个实现。

use crate::protocol::{USER_STORE_VALUE, cow_pattern};

/// #PF error code 的位：P（bit 0）、W/R（bit 1）、US（bit 2）。
const PF_P: u64 = 1 << 0;
const PF_W: u64 = 1 << 1;
const PF_US: u64 = 1 << 2;
/// 场景要求的三位：present + write + user。
const PF_COW_WRITE: u64 = PF_P | PF_W | PF_US;
const PAGE_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    Prima,
    Linux,
}

/// 一次 `cow-write-fault` 场景在某一平台上的归一化观测。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CowObservation {
    pub(crate) platform: Platform,
    /// 目标页上记录到的 #PF 次数（两侧均要求 ≥1）。Prima 的单条后置见证无法观测
    /// 故障重数，故不比较“恰好一次”。
    pub(crate) fault_count: u32,
    /// 故障地址的页内偏移。
    pub(crate) page_offset: u64,
    /// 硬件 error code（低 32 位有效）。
    pub(crate) err: u64,
    /// 分类名。Prima 读自见证；Linux 由 error code 按同一规则推导。
    pub(crate) class: String,
    /// 重试写成功后读回的值等于写入值。
    pub(crate) write_retried_ok: bool,
    /// 原副本未被改写（Prima：共享帧首 qword 仍为 pattern(0)；Linux：父进程读回 pattern(0)）。
    pub(crate) original_unchanged: bool,
    /// 目标页其余内容未被破坏（第 1 个 qword 仍为 pattern(1)）。
    pub(crate) page_rest_intact: bool,
    /// 故障记录先于“重试成功”记录出现。
    pub(crate) order_ok: bool,
}

/// 与 `kernel/src/fault.rs::classify` 同一分类规则（仅本场景关心的分支）。
pub(crate) fn classify(err: u64) -> &'static str {
    if err & PF_P == 0 {
        "not_present"
    } else if err & PF_W != 0 {
        "write_protection"
    } else {
        "other"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Divergence {
    /// 至少一侧没有记录到目标页的故障。
    MissingFault { prima: u32, linux: u32 },
    /// error code 的 P|W|US 三位不是两侧同为 `0x7`。
    ErrBitsMismatch { prima: u64, linux: u64 },
    /// 分类名不一致。
    ClassMismatch { prima: String, linux: String },
    /// 故障地址的页内偏移不一致。
    PageOffsetMismatch { prima: u64, linux: u64 },
    /// “写生效 / 原副本不变 / 页内其余内容完好”在至少一侧不成立。
    ResolutionMismatch {
        prima: Resolution,
        linux: Resolution,
    },
    /// 故障记录未先于解析结果（事件顺序不满足偏序 `fault ≺ resolve ≺ retry_ok`）。
    OrderViolation { prima: bool, linux: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resolution {
    write_retried_ok: bool,
    original_unchanged: bool,
    page_rest_intact: bool,
}

impl Resolution {
    fn of(observation: &CowObservation) -> Self {
        Self {
            write_retried_ok: observation.write_retried_ok,
            original_unchanged: observation.original_unchanged,
            page_rest_intact: observation.page_rest_intact,
        }
    }

    fn ok(self) -> bool {
        self.write_retried_ok && self.original_unchanged && self.page_rest_intact
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct CompatVerdict {
    pub(crate) divergences: Vec<Divergence>,
}

impl CompatVerdict {
    pub(crate) fn is_pass(&self) -> bool {
        self.divergences.is_empty()
    }

    pub(crate) fn describe(&self) -> String {
        if self.is_pass() {
            return "cow-write-fault: no divergence".to_owned();
        }
        self.divergences
            .iter()
            .map(|divergence| format!("{divergence:?}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// 语义化比较两侧观测，返回全部检测到的差异（空即通过）。
pub(crate) fn compare_cow_write_fault(
    prima: &CowObservation,
    linux: &CowObservation,
) -> CompatVerdict {
    let mut divergences = Vec::new();

    if prima.fault_count == 0 || linux.fault_count == 0 {
        divergences.push(Divergence::MissingFault {
            prima: prima.fault_count,
            linux: linux.fault_count,
        });
    }

    if prima.err & PF_COW_WRITE != PF_COW_WRITE || linux.err & PF_COW_WRITE != PF_COW_WRITE {
        divergences.push(Divergence::ErrBitsMismatch {
            prima: prima.err,
            linux: linux.err,
        });
    }

    if prima.class != linux.class {
        divergences.push(Divergence::ClassMismatch {
            prima: prima.class.clone(),
            linux: linux.class.clone(),
        });
    }

    if prima.page_offset != linux.page_offset {
        divergences.push(Divergence::PageOffsetMismatch {
            prima: prima.page_offset,
            linux: linux.page_offset,
        });
    }

    let prima_resolution = Resolution::of(prima);
    let linux_resolution = Resolution::of(linux);
    if !prima_resolution.ok() || !linux_resolution.ok() {
        divergences.push(Divergence::ResolutionMismatch {
            prima: prima_resolution,
            linux: linux_resolution,
        });
    }

    if !prima.order_ok || !linux.order_ok {
        divergences.push(Divergence::OrderViolation {
            prima: prima.order_ok,
            linux: linux.order_ok,
        });
    }

    CompatVerdict { divergences }
}

/// 从 Prima 串口输出中提取 `user_cow_ready` 事件并归一化。
pub(crate) fn parse_prima(output: &str) -> Result<CowObservation, String> {
    let data = output
        .lines()
        .filter(|line| line.contains("event=user_cow_ready"))
        .find_map(|line| {
            line.split('|')
                .find_map(|field| field.strip_prefix("data="))
        })
        .ok_or_else(|| "Prima output has no user_cow_ready event".to_owned())?;

    let mut va = None;
    let mut err = None;
    let mut class = None;
    let mut shared0 = None;
    let mut private0 = None;
    let mut private1 = None;
    let mut receipt = None;

    for field in data.split(';') {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key {
            "va" => va = Some(parse_hex(value)?),
            "err" => err = Some(parse_hex(value)?),
            "class" => class = Some(value.to_owned()),
            "shared0" => shared0 = Some(parse_hex(value)?),
            "private0" => private0 = Some(parse_hex(value)?),
            "private1" => private1 = Some(parse_hex(value)?),
            "receipt" => receipt = Some(parse_hex(value)?),
            _ => {}
        }
    }

    let va = va.ok_or_else(|| "user_cow_ready is missing va".to_owned())?;
    let err = err.ok_or_else(|| "user_cow_ready is missing err".to_owned())?;
    let class = class.ok_or_else(|| "user_cow_ready is missing class".to_owned())?;
    let shared0 = shared0.ok_or_else(|| "user_cow_ready is missing shared0".to_owned())?;
    let private0 = private0.ok_or_else(|| "user_cow_ready is missing private0".to_owned())?;
    let private1 = private1.ok_or_else(|| "user_cow_ready is missing private1".to_owned())?;
    let receipt = receipt.ok_or_else(|| "user_cow_ready is missing receipt".to_owned())?;

    Ok(CowObservation {
        platform: Platform::Prima,
        fault_count: 1,
        page_offset: va % PAGE_SIZE,
        err,
        // Prima 见证自带分类名（由内核 `fault::classify` 产出）；比较时作为字符串核对。
        class,
        write_retried_ok: private0 == USER_STORE_VALUE && receipt == USER_STORE_VALUE,
        original_unchanged: shared0 == cow_pattern(0),
        page_rest_intact: private1 == cow_pattern(1),
        // 单条后置见证无法观测顺序；整页复制（private1）与重试生效（receipt）共同
        // 证明解析先于重试完成，故记为真。
        order_ok: true,
    })
}

/// 解析提交入库的 Linux 归一化基线 trace。
pub(crate) fn parse_linux_trace(fixture: &str) -> Result<CowObservation, String> {
    let mut scenario = None;
    let mut fault = None;
    let mut write_retried_ok = None;
    let mut original_unchanged = None;
    let mut page_rest_intact = None;
    let mut order_ok = None;

    for line in fixture.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("fault ") {
            fault = Some(parse_linux_fault(value)?);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("malformed baseline line: {line}"));
        };
        match key {
            "scenario" => scenario = Some(value.to_owned()),
            "write_retried_ok" => write_retried_ok = Some(parse_flag(value)?),
            "original_unchanged" => original_unchanged = Some(parse_flag(value)?),
            "page_rest_intact" => page_rest_intact = Some(parse_flag(value)?),
            "order_ok" => order_ok = Some(parse_flag(value)?),
            _ => return Err(format!("unknown baseline key: {key}")),
        }
    }

    match scenario.as_deref() {
        Some("cow-write-fault") => {}
        Some(other) => return Err(format!("unexpected baseline scenario: {other}")),
        None => return Err("baseline is missing scenario".to_owned()),
    }

    let (fault_count, page_offset, err) =
        fault.ok_or_else(|| "baseline is missing fault".to_owned())?;

    Ok(CowObservation {
        platform: Platform::Linux,
        fault_count,
        page_offset,
        err,
        class: classify(err).to_owned(),
        write_retried_ok: write_retried_ok
            .ok_or_else(|| "baseline is missing write_retried_ok".to_owned())?,
        original_unchanged: original_unchanged
            .ok_or_else(|| "baseline is missing original_unchanged".to_owned())?,
        page_rest_intact: page_rest_intact
            .ok_or_else(|| "baseline is missing page_rest_intact".to_owned())?,
        order_ok: order_ok.ok_or_else(|| "baseline is missing order_ok".to_owned())?,
    })
}

fn parse_linux_fault(value: &str) -> Result<(u32, u64, u64), String> {
    let mut count = None;
    let mut offset = None;
    let mut err = None;
    for field in value.split(';') {
        let Some((key, value)) = field.split_once('=') else {
            return Err(format!("malformed fault field: {field}"));
        };
        match key {
            "count" => count = Some(value.parse::<u32>().map_err(|error| error.to_string())?),
            "offset" => offset = Some(parse_hex(value)?),
            "error_code" => err = Some(parse_hex(value)?),
            _ => return Err(format!("unknown fault field: {key}")),
        }
    }
    let count = count.ok_or_else(|| "fault is missing count".to_owned())?;
    let offset = offset.ok_or_else(|| "fault is missing offset".to_owned())?;
    let err = err.ok_or_else(|| "fault is missing error_code".to_owned())?;
    Ok((count, offset, err))
}

fn parse_flag(value: &str) -> Result<bool, String> {
    match value {
        "1" => Ok(true),
        "0" => Ok(false),
        other => Err(format!("flag must be 0 or 1, got {other}")),
    }
}

fn parse_hex(value: &str) -> Result<u64, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("expected 0x-prefixed hex, got {value}"))?;
    u64::from_str_radix(digits, 16).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMA_OUTPUT: &str = concat!(
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=frames=0x10;reserved=0x2\r\n",
        "PRIMA|v=1|seq=12|event=user_cow_ready|status=ok|data=va=0x00000a0000002000;",
        "err=0x7;class=write_protection;shared0=0xc0ffee0000000000;",
        "private0=0x5a5a1234deadbeef;private1=0xc0ffee0000000001;",
        "receipt=0x5a5a1234deadbeef\r\n",
    );

    const LINUX_BASELINE: &str = concat!(
        "# prima-linux-trace v1 scenario=cow-write-fault\n",
        "scenario=cow-write-fault\n",
        "fault count=1;offset=0x0;error_code=0x7\n",
        "write_retried_ok=1\n",
        "original_unchanged=1\n",
        "page_rest_intact=1\n",
        "order_ok=1\n",
    );

    fn prima() -> CowObservation {
        parse_prima(PRIMA_OUTPUT).expect("prima output parses")
    }

    fn linux() -> CowObservation {
        parse_linux_trace(LINUX_BASELINE).expect("linux baseline parses")
    }

    #[test]
    fn prima_and_linux_agree() {
        let verdict = compare_cow_write_fault(&prima(), &linux());
        assert!(verdict.is_pass(), "unexpected divergence: {verdict:?}");
    }

    #[test]
    fn prima_class_matches_its_error_code() {
        let observation = prima();
        assert_eq!(observation.class, classify(observation.err));
        assert_eq!(observation.page_offset, 0);
        assert_eq!(observation.class, "write_protection");
    }

    #[test]
    fn embedded_prima_class_wins_over_error_code() {
        let tampered = PRIMA_OUTPUT.replace("class=write_protection", "class=not_present");
        let observation = parse_prima(&tampered).expect("tampered prima output parses");
        let verdict = compare_cow_write_fault(&observation, &linux());
        assert!(matches!(
            verdict.divergences.as_slice(),
            [Divergence::ClassMismatch { .. }]
        ));
    }

    #[test]
    fn error_code_bit_mismatch_is_reported() {
        let tampered = LINUX_BASELINE.replace("error_code=0x7", "error_code=0x3");
        let observation = parse_linux_trace(&tampered).expect("tampered baseline parses");
        let verdict = compare_cow_write_fault(&prima(), &observation);
        assert!(
            verdict
                .divergences
                .iter()
                .any(|divergence| matches!(divergence, Divergence::ErrBitsMismatch { .. }))
        );
    }

    #[test]
    fn page_offset_mismatch_is_reported() {
        let tampered = LINUX_BASELINE.replace("offset=0x0", "offset=0x8");
        let observation = parse_linux_trace(&tampered).expect("tampered baseline parses");
        let verdict = compare_cow_write_fault(&prima(), &observation);
        assert!(
            verdict
                .divergences
                .contains(&Divergence::PageOffsetMismatch { prima: 0, linux: 8 })
        );
    }

    #[test]
    fn missing_fault_is_reported() {
        let tampered = LINUX_BASELINE.replace("count=1", "count=0");
        let observation = parse_linux_trace(&tampered).expect("tampered baseline parses");
        let verdict = compare_cow_write_fault(&prima(), &observation);
        assert!(
            verdict
                .divergences
                .iter()
                .any(|divergence| matches!(divergence, Divergence::MissingFault { .. }))
        );
    }

    #[test]
    fn resolution_mismatch_is_reported() {
        let tampered = LINUX_BASELINE.replace("original_unchanged=1", "original_unchanged=0");
        let observation = parse_linux_trace(&tampered).expect("tampered baseline parses");
        let verdict = compare_cow_write_fault(&prima(), &observation);
        assert!(
            verdict
                .divergences
                .iter()
                .any(|divergence| matches!(divergence, Divergence::ResolutionMismatch { .. }))
        );
    }

    #[test]
    fn order_violation_is_reported() {
        let tampered = LINUX_BASELINE.replace("order_ok=1", "order_ok=0");
        let observation = parse_linux_trace(&tampered).expect("tampered baseline parses");
        let verdict = compare_cow_write_fault(&prima(), &observation);
        assert!(verdict.divergences.contains(&Divergence::OrderViolation {
            prima: true,
            linux: false
        }));
    }

    #[test]
    fn baseline_rejects_unknown_keys() {
        let tampered = format!("{LINUX_BASELINE}extra=1\n");
        assert!(parse_linux_trace(&tampered).is_err());
    }

    #[test]
    fn prima_output_requires_the_user_cow_event() {
        let error = parse_prima("PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n")
            .expect_err("missing user_cow_ready must fail");
        assert!(error.contains("no user_cow_ready event"));
    }

    #[test]
    fn every_c_linux_dispatch_entry_has_a_current_api_ledger_row() {
        let source = include_str!("../../prima-user/c/prima_syscall.c");
        let ledger = include_str!("../../docs/superpowers/compat/stage5-working-api.md");
        let dispatch = source
            .split_once("static long prima_translate_body(")
            .unwrap()
            .1;
        let dispatch = dispatch.split_once("/* M15：").unwrap().0;
        for case in dispatch.split("case PRIMA_LINUX_SYS_").skip(1) {
            let name = case.split_once(':').unwrap().0;
            assert!(
                ledger.contains(&format!("| `{name}` (")),
                "missing API row: {name}"
            );
        }
    }
}
