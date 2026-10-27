---
id: report-posix-std-backend
kind: report
authority: scoped-evidence
status: validated-with-regression-failure
lifecycle: historical
updated: 2026-10-27
---

# POSIX std 后端实现与验证

基线 `be3d6dd`；从 `107fd2c` 创建 `codex/posix-std-backend` 后同步 origin/master，快进到基线。
范围见[工作契约](../specs/2026-10-27-posix-std-backend-design.md)。
原 A 子集与[前序统一报告](2026-10-25-stage0-5-gap-followup.md)保留；本报告不复用旧 PASS 作为本次证据。

阶段5出口保持未签署，阶段6未启动。后端的模块接入与底层完整POSIX语义分开解释。

## 实现

- 独立 `x86_64-prima-posix`，vendor/os为prima，Unix family与musl env，静态PIE、panic=abort。
  使用标准x86_64 SSE/SSE2浮点ABI与musl匹配；禁用AVX，不要求内核XSAVE支持。
  原R1/R2目标保持softfloat及原范围。
- 固定nightly 1a833e165、std/unwind/backtrace/libc源码树hash；只修改ignored隔离副本，明确Prima的ABI cfg别名。
  libc编译输入统一重定向到同一锁定副本，未修改registry或安装工具链，也没有注入Linux target cfg。
- std Unix模块→musl→Prima适配层；标准main/lang_start和现有crt初始化args/env/TLS。
  默认System分配器是musl malloc/free；使用pthread-key TLS，不生成loader不支持的PT_TLS。
- socket创建走普通socket+fcntl；futex将固定单调绝对期限转换为每次剩余的相对WAIT，
  EINTR重算剩余期限，不重新延长整个等待。默认线程栈64KiB，仍受96页arena约束。
- poll的ENOSYS回退到逐fd fcntl检查；不安装不存在的sigaltstack诊断。
  panic=abort禁用栈回溯且无需链接系统unwinder；保留上游声明以满足构建时panic_unwind依赖，不承诺展开。
- UDP接受MSG_NOSIGNAL（本来不产生SIGPIPE），其它flags仍拒绝；host检查实际适配层输入边界。
- 构建器将crt/适配层对象、libc.a及链接脚本内容摘要纳入Cargo指纹，避免路径不变复用旧ELF。
  `--manifest-path`入口可构建其它标准main程序，强制panic=abort，不需探针专用入口或GlobalAlloc。
  构建overlay摘要是Cargo编译输入，改变源覆盖层后不能复用旧std rlib。

## R3实际覆盖

标准args含空串/UTF-8、初始env与set_var/get_var、cwd；16次64KiB分配/释放（累计超过arena），
HashMap；文件create_new/write/read/seek/fstat/read_dir/rename/unlink/ENOENT；
UDP bind/getsockname/sendto/recvfrom/close；默认std::thread::spawn/join、TLS父子隔离及析构、
Arc/Mutex/Condvar真实阻塞、park_timeout、Instant/sleep、Process id和托管exit。
最终目标另检查运行时f64::sin经musl/libm浮点ABI。stdout为44B、FNV-1a=1614292895。R3清理后的tasks=12、caps_live=134，与严格TASKCAP守卫一致。

R3在Prima侧还检查fsync返回ENOSYS、Command::status与TCP连接返回错误、CommandExt::exec真实VFS查找的ENOENT，
没有把这些负例当作对应功能已经实现。Linux同源程序覆盖共同支持路径。
原生错误路径的明确作用域保留，不能用Linux结果代替机器结果。

## 本次测试与证据

日志保留在当前worktree的 `target/prima-os/` 和本轮 `/tmp/prima-posix-*.log`。

| 命令 | 本次结果与范围 |
|---|---|
| cargo test --workspace | PASS1016；最终重跑日志workspace-final |
| cargo xtask lint | PASS fmt、Clippy -Dwarnings、裸机kernel及32文件零unsafe；lint-final |
| python3 prima-user/musl/test_port.py | PASS5项；含MSG_NOSIGNAL接受及未知组合拒绝 |
| Rust原生core/alloc及restricted std Clippy | 两模式PASS，r1-clippy/r2-clippy |
| python3 scripts/build_posix_std.py --clippy | 最终SSE目标PASS，clippy-final-pass（含CommandExt::exec的ENOENT） |
| 独立普通main示例（无自定义入口/分配器、无LTO） | 构建PASS；ELF检查Prima声明、无INTERP/PT_TLS、仅RELATIVE重定位；generic-port |
| R1/R2/R3机器验收 | 最终全量命令中的三组各两轮PASS；R3为最终SSE/普通main/ExitCode版本 |
| cargo xtask test | 35组均执行；69/70轮验收PASS；I1第二轮IPC比较失败，最终退出1；R3两轮PASS |
| cargo xtask compat | PASS cow-write-fault |
| cargo xtask tcb | PASS trusted2072/mechanism30158/total32230，6.43%；unsafe78/0 |
| docs self-test/build/check/check --base be3d6dd | PASS318份；最终报告与阶段跟踪更新后重新生成，四项均PASS |

## 本次失败及修复链

1. workspace首次在沙箱中遇到bootloader子构建rsproxy.cn解析失败；授权沙箱外重跑PASS。
2. 初次std路径只替换直接libc依赖，遗漏其它build-std依赖边，导致registry libc缺Prima类型、178项编译错误。
   改为统一编译输入重定向后消除；Cargo源码副本而非registry被修改。
3. 初次静态链接缺gcc_s/gcc；显式静态CRT及libgcc目录后又暴露不必要的unwind链接。
   初次裁掉整个unwind crate导致隐式std/prelude缺失，随后空crate又导致panic_unwind依赖缺声明。
   最终只禁用Prima的系统unwinder链接选择，保留声明，panic=abort编译链接通过。
4. socket最初移除整文件Linux cfg选择，使其它Linux ABI扩展方法缺失（12项编译错误）；
   改为仅在socket创建分支选择普通socket+fcntl，保留其它模块声明。
5. R3 Linux参考首次因沙箱UDP的EPERM失败；沙箱外同源程序PASS，不归为OS缺陷。
6. R3首次机器运行在UDP send_to处EINVAL后panic，并触发原30秒门限超时。
   根因为std传MSG_NOSIGNAL而适配层仅接受flags=0；修正后R3两轮PASS。
7. pthread-key TLS的上游宏使Clippy在显式const初始化时仍报missing_const_for_thread_local；
   改为用真实Process id运行时初始化，并检查析构时身份后PASS，不加allow或放宽警告。
8. 独立普通main示例最初仍被上游restricted_std目标白名单拒绝；在锁定std/build.rs中显式登记Prima，
   初次修改std/build.rs后Cargo仍复用了旧host构建脚本，继续输出restricted_std（57项门禁错误）。
   将patched构建脚本源路径绑定overlay摘要后，host构建脚本也被重新编译。
   普通应用无需探针的feature(restricted_std)。该改变只在新的POSIX sysroot内，原A仍受限。
   后端构建采用文件锁，隔离副本重建及C对象写入不能与另一构建交错。
   R3的临时文件放在新建的进程专用目录内，拒绝目录碰撞，避免host参考覆盖公共/tmp文件。
9. 独立示例不启用LTO时还暴露std基础栈回溯对_Unwind_GetIP/_Unwind_Backtrace的引用；
   初次取消backtrace_in_libstd配置导致backtrace库误选外部std/gimli依赖（11项编译错误）；
   最终保留该集成配置，在锁定backtrace源码中为Prima选择上游noop实现，与panic=abort/无unwinder契约一致。
   不通过只测试启用LTO的探针隐藏普通程序的链接缺口。

10. 完整命令中I1第二轮正常退出，但被IPC对比拒绝：第一轮(queued,direct,blocks)=(84,339,354)，
    第二轮=(83,340,353)。两轮消息总数均423，但阻塞数不同；保留 `/tmp/prima-posix-full.log` 的完整失败输出，
    不放宽validator，不把exit=0写成该轮验收PASS。差异路径仍待定位，其它组按驱动继续执行。
    最终69/70轮验收PASS、35组均执行，命令退出1；R3两轮、其它原组均PASS。
11. 文档初检发现已撤回的探针build.rs仍在主题code路由中；移除残留项后重新build/check通过。

没有通过延长30秒门限、跳过旧组、自动重试或放松stdout/退出/资源检查处理失败。
最终全量与开发版本结果独立登记，不合并不同源版本冒充单次全绿。

## 支持限制

这是当前POSIX profile上的std后端接入。Command子进程创建（vfork/pipe等）、TCP/IPv6/DNS、
CLOCK_REALTIME、安全熵、持久化、文件fsync/ftruncate/pread/pwrite等仍取决于底层未实现能力；
不能称为完整POSIX符合性或无边界的全量std支持。SystemTime::now及随机字节保留明确panic边界。
HashMap使用上游无熵平台的地址种子fallback，不提供抗HashDoS保证。
线程上限/内存预算/单CPU、无sigaltstack及无panic unwind边界保留；没有SMP/Miri/Lean新证明。
OS线程ID诊断gettid尚不在底层支持集；Prima后端返回None，不把-1转换成虚假的u64线程ID。



## 继续实现与整改

[文件I/O与IPC接收归账后继](2026-10-27-posix-std-file-io-report.md)继续实现ftruncate/pread/pwrite及Rust对应API，
并以完整阻塞出口归账修订I1验收假设。本报告的69/70失败及未支持项属于上述基线，
后继实际支持和实跑结果以该报告明确范围为准，原失败不回写为PASS。
