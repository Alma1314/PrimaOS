# SPEC-092 报告：独立第二服务端点（init 按名路由到不同任务）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[多端点服务设计](../specs/2026-10-11-stage4-multi-endpoint-service-design.md)
>
> 前置：[SPEC-091 vspace 存储移出](2026-10-11-stage4-vspace-arena-report.md)、[SPEC-090 服务管理器](2026-10-11-stage4-service-manager-report.md)

## 1. 本片要回答的问题

SPEC-090 §6.1 登记「两个注册服务由同一宿主 ring3 任务承载，独立多端点任务待后续切片」。
SPEC-091 解除了其存储墙。本片落地该待办：新增**完全独立的第二个 ring3 Rust 服务任务**，
init 按名把请求路由到不同任务，并让判决性见证区分**端点身份**而非仅行为码。

## 2. 判决性见证（`service_selected_ready`，seq 28 扩展）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;
alpha_ep=0x9;beta_ep=0xa;beta_self=0xa
```

- `alpha_ep=9` / `beta_ep=10`：两个**相异** task id——两个独立任务、独立根、独立内核栈。
- `beta_self=10`：init 从第二个 `SERVICE_ID_TAG` 收集到的端点 id，等于写回结果的 `beta_ep`——
  把「结果」绑定到「被路由到的那个端点」。
- `negotiated_beta = 0x0001_0000_0400_0001`：`max=1024` 只可能来自 `BETA` offer（B 在自己根下
  映射段、独立协商），且与 SPEC-090 逐字节一致。

## 3. 定位并修复的缺陷（段 cap 的派生权限与时序）

第一版实测 **QEMU 卡死**（`SYS_RECV: no Ready task to switch to`，panic 于 `syscall.rs:1528`）。用
内核侧 send/derive/map 追踪定位到：第二个端点 B 已创建、已自报 id、init 已把 `prepare(BETA)` 与
段 cap 投递给它（send 均返回成功），但 **B 的 `memseg_map` 返回 `MissingRight`**，故 B 不协商、
不回 `DONE`，init 的 done 循环阻塞致死。

**根因**：`bootstrap::memseg_map` 的前置是 `authorize(caller, seg_cap, Right::Derive)`——**段 cap 必须
含 `Derive`**。我为 B 派生的 cap 只给了 `{SegMapR, SegMapW}`（无 `Derive`），故被拒。此外该派生
一度放在转发之后，而转发会**移动**原 cap，init 已不再持有。

**修复**：派生权限改为 `MEMSEG_RW_BITS | ACCESS_DERIVE_BITS`，并把派生移到「令牌派生之后、
转发之前」（见设计 §2.2/§5）。

> 该缺陷只有 **QEMU 实测**才暴露（host 单测与合成反例均不受影响），再次印证「机器码/授权前置
> 类错误需真实运行钉住」。

## 4. 回归面（手工事件流 diff，逐条登记；基线 = SPEC-091 HEAD）

42 条事件、seq **无增删**；rel diff（vs SPEC-091 终点）如下：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready`（3） | `usable`/`image`/`kend` | 30174/1290/0x150a000 → **30167/1293/0x150d000** |
| `bootstrap_ready`（9） | `image_len`/`image_frames`/`free_frames` | 5282768/1290/30174 → **5294584/1293/30167** |
| `ipc_ready`（23） | `queued`/`blocks` | 6/11 → **8/12**（`direct` 9 不变） |
| `task_ready`（24） | `tasks`/`tasks_after`/`caps`/`caps_after` | 5/5/54/54 → **6/6/60/60** |
| `preempt_ready`（30） | `rsp*`/`kstack*_top` | 前移（`root*`/`cr3*`/`data*`/`mark*`/`ticks` 不变） |
| `frames_ready`（36） | `universe`/`free`/`table`/`image` | 31464/30038/105/1290 → **31460/30024/112/1293**（`segment=31` 不变） |
| `self_root_ready`（37） | `pages` | 458 → **464** |
| `state_audit_ready`（39） | `total`/`to_running`/`to_ready`/`to_blocked`/`used` | 84/35/32/13/234 → **88/37/33/14/248** |
| `service_selected_ready`（28） | 新增 `alpha_ep`/`beta_ep`/`beta_self` | 见 §2 |

**判决性的「不变」**：seq 27 `service_negotiated_ready` 的 `token=0x10000001c`/`service=0x10000001d`
**未顺移**（见设计 §5 的槽序规避）；`platform_segments_ready`（34）逐字段不变。

同镜像连跑两次 42 条事件逐字节相同。

## 5. 验证（2026-10-11）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **366**、prima-ostd 21、xtask **200**（+2 反例） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20676 total_lines=21938 ratio=5.75%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**42 条事件**，含 seq 28 扩展见证） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例（xtask 单位测试，对合成输入）：
`rejects_service_selected_when_beta_ep_is_not_the_second_task`（把 `beta_ep` 换成 `alpha_ep`=9 即失败）、
`rejects_service_selected_when_beta_self_is_not_the_routed_endpoint`（`beta_self` 与 `beta_ep` 脱钩即失败），
连同 SPEC-090 的三条（错误行为/硬编码名/未注册名不拒绝）。

## 6. 未决与登记

1. **同一 ELF、共享物理栈帧**：两端点由同一嵌入 ELF 创建（独立任务/根/内核栈），沿用 SPEC-088 的
   共享栈帧；**不同二进制的多服务**与独立栈帧待后续切片。这是**如实登记的简化**。
2. 注册表仍为编译期常数；名字为 64 位码而非字符串路径（namespace 的名字→cap 解析是另一条线）。
3. **S4-D2 与阶段 4 交付项/退出条件一律不勾选**；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
