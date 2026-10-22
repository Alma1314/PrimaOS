# 阶段 5 / M17c-1：静态 ELF 解析与初始栈（host 证据）

**日期**：2026-10-23
**范围**：`kernel/src/exec.rs` 的纯逻辑片：ELF64 加载计划与
`argc/argv/envp/auxv` 初始栈构造。**未接 syscall/地址空间替换**。
**结论**：4 个 host 用例通过；`cargo test --workspace` 全绿、lint 全 PASS。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/exec.rs` | `plan(image)`：校验 ELF64/`ET_EXEC`/`EM_X86_64`、拒绝 `PT_INTERP`（动态）、最多 4 个 `PT_LOAD`、检查 `vaddr` 页对齐/`memsz>=filesz`/段在镜像内；输出 `ExecPlan`（entry + 段描述） |
| | `build_stack(top, argv, envp, auxv, buffer)`：自顶向下构造初始栈——`argc`、argv/envp 指针 + NULL、auxv 对 + `AT_NULL`、字符串（NUL 结尾、8 字节对齐），返回 `rsp`/`argc` |
| host 用例 | `plan_accepts_static_elf_with_two_load_segments`、`plan_rejects_dynamic_pie_and_bad_headers`、`stack_layout_places_argc_argv_envp_auxv_and_strings`、`stack_rejects_tiny_buffers` |

## 2. 证据

```text
cargo test -p kernel exec              → 4 passed
cargo test --workspace                 → 全绿
cargo xtask lint / fmt                 → PASS
```

## 3. 边界

- 只支持静态 `ET_EXEC`（无 PIE/解释器/动态链接）；
- `build_stack` 的字符串/数组上限（16 项 argv/envp）为探针/最小 exec 设定；
- 未做：把段字节写入新地址空间、拆旧映射、设置入口帧、`CLOEXEC` 关闭、
  `SYS_EXECVE` 胶水与 W7。

## 4. 下一步

1. 内核 `SYS_EXEC_REPLACE`（或管理器侧）接线：拆旧 vspace、按 plan 映射新段、
   把 `build_stack` 结果拷入新栈、设置 `rip=e_entry`/`rsp`；
2. 适配层 `execve`（Linux 59）读 VFS 文件、构造 argv/envp、调用内核；
3. W7 探针（argv/envp、CLOEXEC）双轮 + Linux 同源参考；
4. 本报告不代表 execve 可用或阶段 5 退出条件满足。
