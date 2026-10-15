//! 切片 AI（SPEC-080）收口门禁与可信面（TCB）量化指标。
//!
//! `lint` 子命令把 framekernel 的边界变成**可复跑的门禁**：`fmt --check`、`clippy -D warnings`、
//! 内核在 `deny(unsafe_code)` 下编到裸机 target，以及**源码级断言内核零 `unsafe`**（含测试代码）。
//! `tcb` 子命令实现差距分析 **G3** 要求的可证伪度量：可信代码行占比，用于观察其增长趋势。
//!
//! 口径：**可信面（TCB）= `prima-ostd/src`**（唯一被允许写 `unsafe` 的 crate）；
//! **对照面 = `kernel/src`**（机制层，零 `unsafe`）。二者即本项目全部 OS 实现代码
//! （`cosir`/`cosir_state` 是可执行模型、`xtask` 是构建工具，都不进内核镜像，故不计入）。

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const TRUSTED_DIR: &str = "prima-ostd/src";
const MECHANISM_DIR: &str = "kernel/src";
const KERNEL_TARGET: &str = "x86_64-unknown-none";

/// `cargo xtask lint`：全部收口门禁。
pub fn lint_command(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err(usage());
    }
    let root = workspace_root()?;

    run_cargo(&root, &["fmt", "--all", "--check"])?;
    println!("PASS fmt --all --check");

    run_cargo(
        &root,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    println!("PASS clippy -D warnings");

    // 编译期证明：内核（lib + bin）在 `#![deny(unsafe_code)]` 下可编到裸机 target。
    run_cargo(&root, &["build", "-p", "kernel", "--target", KERNEL_TARGET])?;
    println!("PASS kernel builds under deny(unsafe_code) for {KERNEL_TARGET}");

    // 源码级证明：内核 crate 里连测试代码都不出现 `unsafe` 标识符，也不出现 allow 豁免。
    let scanned = scan_unsafe(&root.join(MECHANISM_DIR))?;
    if !scanned.hits.is_empty() {
        return Err(format!(
            "kernel crate must contain no `unsafe`: {} hit(s): {}",
            scanned.hits.len(),
            scanned.hits.join(", ")
        ));
    }
    if scanned.allowances > 0 {
        return Err(format!(
            "kernel crate must contain no `#[allow(unsafe_code)]`: {} found",
            scanned.allowances
        ));
    }
    println!(
        "PASS kernel {MECHANISM_DIR} has zero `unsafe` ({} files scanned)",
        scanned.files
    );
    Ok(())
}

/// `cargo xtask tcb`：打印可信面占比（G3 的可证伪度量）。
pub fn tcb_command(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err(usage());
    }
    let root = workspace_root()?;
    let report = tcb_report(&root)?;
    println!(
        "TCB trusted_lines={} mechanism_lines={} total_lines={} ratio={:.2}%",
        report.trusted_lines,
        report.mechanism_lines,
        report.total_lines(),
        report.ratio_percent()
    );
    println!(
        "TCB unsafe_tokens trusted={} mechanism={}",
        report.trusted_unsafe, report.mechanism_unsafe
    );
    if report.mechanism_unsafe != 0 {
        return Err(format!(
            "mechanism layer must carry zero `unsafe` tokens, found {}",
            report.mechanism_unsafe
        ));
    }
    let coupling = kernel_service_coupling(&root)?;
    println!(
        "TCB kernel_service_coupling include_bytes={} prima_user_dep={} service_name_lines={} protocol_lines={}",
        coupling.include_bytes,
        coupling.prima_user_dep,
        coupling.service_name_lines,
        coupling.protocol_lines
    );
    if coupling.include_bytes != 0 {
        return Err(format!(
            "EXO-1 regression: kernel/src contains {} `include_bytes!`",
            coupling.include_bytes
        ));
    }
    if coupling.prima_user_dep {
        return Err(
            "EXO-1 regression: kernel/Cargo.toml depends on `prima-user` artifact".to_owned(),
        );
    }
    println!("PASS tcb metric");
    Ok(())
}

/// EXO-1 起持续约束的“内核-服务耦合面”计数（只报数/守住 0 回归）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct KernelServiceCoupling {
    include_bytes: usize,
    prima_user_dep: bool,
    service_name_lines: usize,
    protocol_lines: usize,
}

fn kernel_service_coupling(root: &Path) -> Result<KernelServiceCoupling, String> {
    let cargo = fs::read_to_string(root.join("kernel/Cargo.toml"))
        .map_err(|error| format!("failed to read kernel/Cargo.toml: {error}"))?;
    let mut coupling = KernelServiceCoupling {
        prima_user_dep: cargo.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("prima-user") || line.contains("package = \"prima-user\"")
        }),
        ..KernelServiceCoupling::default()
    };
    let dir = root.join("kernel/src");
    let entries =
        fs::read_dir(&dir).map_err(|error| format!("failed to read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to walk {}: {error}", dir.display()))?;
        if entry.path().extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let text = fs::read_to_string(entry.path())
            .map_err(|error| format!("failed to read {}: {error}", entry.path().display()))?;
        for line in text.lines() {
            if line.contains("include_bytes!") {
                coupling.include_bytes += 1;
            }
            if line.contains("SERVICE_NAME_") || line.contains("service_name(b\"") {
                coupling.service_name_lines += 1;
            }
            if line.contains("SERVICE_") || line.contains("SERVICEMGR_") {
                coupling.protocol_lines += 1;
            }
        }
    }
    Ok(coupling)
}

/// 可信面/机制层的行数与 `unsafe` 计数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcbReport {
    pub trusted_lines: usize,
    pub mechanism_lines: usize,
    pub trusted_unsafe: usize,
    pub mechanism_unsafe: usize,
}

impl TcbReport {
    pub fn total_lines(&self) -> usize {
        self.trusted_lines + self.mechanism_lines
    }

    /// 可信代码行占（可信面 + 机制层）的百分比。总数不可能为 0（两个目录都非空）。
    pub fn ratio_percent(&self) -> f64 {
        self.trusted_lines as f64 * 100.0 / self.total_lines() as f64
    }
}

fn tcb_report(root: &Path) -> Result<TcbReport, String> {
    let trusted = scan_unsafe(&root.join(TRUSTED_DIR))?;
    let mechanism = scan_unsafe(&root.join(MECHANISM_DIR))?;
    Ok(TcbReport {
        trusted_lines: trusted.code_lines,
        mechanism_lines: mechanism.code_lines,
        trusted_unsafe: trusted.hits.len(),
        mechanism_unsafe: mechanism.hits.len(),
    })
}

/// 递归扫描目录下全部 `.rs`，统计代码行、`unsafe` 标识符与 allow 豁免。
fn scan_unsafe(directory: &Path) -> Result<Scan, String> {
    let mut files = Vec::new();
    collect_rust_files(directory, &mut files)?;
    files.sort();

    let mut scan = Scan::default();
    for file in &files {
        let source = fs::read_to_string(file)
            .map_err(|error| format!("failed to read {file:?}: {error}"))?;
        let report = scan_source(&source);
        scan.files += 1;
        scan.code_lines += report.code_lines;
        scan.allowances += report.allowances;
        for line in report.unsafe_lines {
            scan.hits.push(format!("{}:{line}", file.display()));
        }
    }
    Ok(scan)
}

#[derive(Default)]
struct Scan {
    files: usize,
    code_lines: usize,
    allowances: usize,
    hits: Vec<String>,
}

/// 单文件扫描结果（测试可直接驱动）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceReport {
    pub code_lines: usize,
    pub unsafe_lines: Vec<usize>,
    pub allowances: usize,
}

/// 扫描一段 Rust 源码：剥离注释与字符串后统计。
pub fn scan_source(source: &str) -> SourceReport {
    let stripped = strip_comments(source);
    let mut code_lines = 0;
    let mut unsafe_lines = Vec::new();
    let mut allowances = 0;
    for (index, line) in stripped.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        code_lines += 1;
        if line.contains("allow(unsafe_code)") {
            allowances += 1;
        }
        if contains_word(line, "unsafe") {
            unsafe_lines.push(index + 1);
        }
    }
    SourceReport {
        code_lines,
        unsafe_lines,
        allowances,
    }
}

/// `line` 中是否出现**独立标识符** `word`（前后不得是标识符字符）。
///
/// 用于把 lint 名（`unsafe_code`、`unsafe_op_in_unsafe_fn`）与 `unsafe` 关键字区分开。
fn contains_word(line: &str, word: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let needle: Vec<char> = word.chars().collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return false;
    }
    for start in 0..=chars.len() - needle.len() {
        if chars[start..start + needle.len()] != needle[..] {
            continue;
        }
        let before_ok = start == 0 || !is_identifier_char(chars[start - 1]);
        let after_ok = chars
            .get(start + needle.len())
            .is_none_or(|next| !is_identifier_char(*next));
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// 剥离 `//` 行注释与 `/* */` 块注释；字符串字面量的内容同样被清空（避免字符串里的
/// `unsafe`/`//` 造成误判）。**换行一律保留**，故行号在剥离前后一致。
pub fn strip_comments(source: &str) -> String {
    #[derive(PartialEq)]
    enum State {
        Normal,
        Str,
        Line,
        Block(u32),
    }

    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut state = State::Normal;
    let mut index = 0;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        match state {
            State::Normal => {
                if current == '"' {
                    state = State::Str;
                    out.push(' ');
                } else if current == '/' && next == Some('/') {
                    state = State::Line;
                    index += 1;
                } else if current == '/' && next == Some('*') {
                    state = State::Block(1);
                    index += 1;
                } else if current == '\'' && char_literal_len(&chars, index).is_some() {
                    // 字符字面量（可能是 `'"'`）整体跳过：否则其中的引号会被误判为字符串起点。
                    index += char_literal_len(&chars, index).expect("checked above") - 1;
                } else {
                    out.push(current);
                }
            }
            State::Str => {
                if current == '\\' {
                    index += 1;
                } else if current == '"' {
                    state = State::Normal;
                } else if current == '\n' {
                    out.push('\n');
                }
            }
            State::Line => {
                if current == '\n' {
                    out.push('\n');
                    state = State::Normal;
                }
            }
            State::Block(depth) => {
                if current == '\n' {
                    out.push('\n');
                } else if current == '/' && next == Some('*') {
                    state = State::Block(depth + 1);
                    index += 1;
                } else if current == '*' && next == Some('/') {
                    state = if depth == 1 {
                        State::Normal
                    } else {
                        State::Block(depth - 1)
                    };
                    index += 1;
                }
            }
        }
        index += 1;
    }
    out
}

/// 若 `chars[start]` 是字符字面量的起点，返回其总长度（含首尾引号）。
///
/// 用于把 `'"'` 这类字面量整体跳过。生命周期（`'a`）没有闭合引号，故返回 `None`。
fn char_literal_len(chars: &[char], start: usize) -> Option<usize> {
    if chars.get(start) != Some(&'\'') {
        return None;
    }
    match chars.get(start + 1)? {
        '\\' => {
            // 转义形：跳过 `\` 与其后的转义字符，再找闭合引号（`'\n'`、`'\''`、`'\u{...}'`）。
            let mut index = start + 3;
            while index < chars.len() {
                if chars[index] == '\'' {
                    return Some(index - start + 1);
                }
                index += 1;
            }
            None
        }
        _ if chars.get(start + 2) == Some(&'\'') => Some(3),
        _ => None,
    }
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to list {directory:?}: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("cannot locate the workspace root above {manifest:?}"))
}

fn run_cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new("cargo")
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|error| format!("failed to run `cargo {}`: {error}", args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` failed: {status}", args.join(" ")))
    }
}

fn usage() -> String {
    "usage: cargo xtask <lint|tcb>".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments_removes_line_and_nested_block_comments() {
        let source = "// unsafe\nlet a = 1; /* unsafe /* unsafe */ */\nlet b = 2;\n";
        let stripped = strip_comments(source);
        assert!(!stripped.contains("unsafe"));
        // 行号保持：三行都还在（首行只剩换行）。
        assert_eq!(stripped.lines().count(), 3);
        assert!(stripped.contains("let a = 1;"));
        assert!(stripped.contains("let b = 2;"));
    }

    #[test]
    fn strip_comments_blanks_string_literal_contents() {
        // 字符串里的 `unsafe` 与 `//` 都不得泄漏成 token 或被当作注释起点。
        let source = "let s = \"unsafe // not a comment\";\nlet x = 1;\n";
        let stripped = strip_comments(source);
        assert!(!stripped.contains("unsafe"));
        assert!(stripped.contains("let x = 1;"));
        // 字符串内的 `//` 起点之后的内容仍属同一行源码行（未被当注释吞掉）。
        assert_eq!(stripped.lines().count(), 2);
    }

    #[test]
    fn scan_source_flags_unsafe_and_allowances() {
        let source = "#![deny(unsafe_code)]\n#[allow(unsafe_code)]\nunsafe { asm!(\"nop\"); }\n";
        let report = scan_source(source);
        // 只有第 2、3 行算代码行（第 1 行也是代码行，共 3 行）。
        assert_eq!(report.code_lines, 3);
        assert_eq!(report.unsafe_lines, vec![3]);
        assert_eq!(report.allowances, 1);
    }

    #[test]
    fn scan_source_ignores_lint_names_that_are_not_the_token() {
        // `unsafe_code` / `unsafe_op_in_unsafe_fn` 是 lint 名，不是 `unsafe` 标识符。
        let source = "#![deny(unsafe_code)]\n#![deny(unsafe_op_in_unsafe_fn)]\n";
        let report = scan_source(source);
        assert!(report.unsafe_lines.is_empty());
        assert_eq!(report.allowances, 0);
    }

    #[test]
    fn scan_source_counts_only_non_blank_non_comment_lines() {
        let source = "// header\n\nlet a = 1;\n   \n/* block */\nlet b = 2;\n";
        assert_eq!(scan_source(source).code_lines, 2);
    }

    #[test]
    fn tcb_report_is_a_ratio_of_the_two_layers() {
        let report = TcbReport {
            trusted_lines: 25,
            mechanism_lines: 75,
            trusted_unsafe: 30,
            mechanism_unsafe: 0,
        };
        assert_eq!(report.total_lines(), 100);
        assert_eq!(report.ratio_percent(), 25.0);
    }

    #[test]
    fn scanned_repository_has_zero_unsafe_in_the_mechanism_layer() {
        let root = workspace_root().expect("workspace root");
        let mechanism = scan_unsafe(&root.join(MECHANISM_DIR)).expect("scan kernel");
        assert!(
            mechanism.hits.is_empty(),
            "kernel must be unsafe-free, found {:?}",
            mechanism.hits
        );
        assert_eq!(mechanism.allowances, 0);
        // 信任面必须真的承载 unsafe（否则边界名不副实）。
        let trusted = scan_unsafe(&root.join(TRUSTED_DIR)).expect("scan framework");
        assert!(!trusted.hits.is_empty(), "trusted crate must carry unsafe");
        assert!(trusted.files > 0);
    }

    #[test]
    fn strip_comments_skips_char_literal_containing_a_quote() {
        // `'"'` 若被当成字符串起点，会把后续源码整段吞掉。
        let source = "let q = '\"';\nlet x = 1;\nunsafe fn f() {}\n";
        let stripped = strip_comments(source);
        assert!(stripped.contains("let x = 1;"));
        assert_eq!(scan_source(source).unsafe_lines, vec![3]);
    }

    #[test]
    fn char_literal_len_distinguishes_lifetimes_and_literals() {
        let chars = |s: &str| s.chars().collect::<Vec<char>>();
        assert_eq!(char_literal_len(&chars("'a' "), 0), Some(3));
        assert_eq!(char_literal_len(&chars("'\\n' "), 0), Some(4));
        assert_eq!(char_literal_len(&chars("'\\'' "), 0), Some(4));
        assert_eq!(char_literal_len(&chars("'a, b"), 0), None); // 生命周期
    }
}
