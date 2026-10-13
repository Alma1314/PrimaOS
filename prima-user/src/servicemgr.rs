//! SPEC-104：**ring3 服务管理器**——运行期注册表、按名解析与请求路由/转发的宿主。
//!
//! 决策见 `docs/superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md`：把服务发现
//! 与路由从 `init` 的手写 x86 机器码迁到本独立 ring3 任务。本映像**不含任何服务名字面量之外的
//! 对端/端点 task id**——端点与客户端都以本管理器为对端（对端 id 由创建时经入口 `r8` 运行期
//! 指派），本管理器按自报消息里的名字建注册表、按客户端写入共享段的请求名解析、并把 `prepare`
//! 与共享段 cap 转发给解析到的端点。
//!
//! 与本映像一起构成判决性见证的，是本管理器写回**客户端共享段**固定偏移的自证字段（自身 task
//! id + 二进制标记）与解析结果——内核见证按共享段偏移读回，故「注册表/解析由独立 ring3 映像
//! 产出」是逐字节可核验的，而非仅凭 init 的回执。
//!
//! 消息处理是**顺序无关的事件循环**：客户端的 Hello 与两端点自报、done 可按任意次序到达，
//! 状态齐全时（Hello + 两端点 + Alpha done）才推进下一阶段。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, SyscallTransport, TaskId,
    protocol::{
        SERVICE_BEHAVIOR_ALPHA, SERVICE_BEHAVIOR_BETA, SERVICE_BEHAVIOR_NONE, SERVICE_NAME_ALPHA,
        SERVICE_NAME_BETA, SERVICE_NAME_MASK, SERVICE_NAME_OFFSET, SERVICEMGR_HELLO_TAG,
        pack_service_name,
    },
};

/// 客户端把共享段映射到的 VA（与本进程自身根内的同一物理帧映射目标；与 `prima-user` 一致）。
const USER_SEG_VA: u64 = 0x0000_0A00_0000_7000;
/// 客户端把 `PONG` 的 payload 写回共享段内的偏移（与 `prima-user`/内核一致）。
const SERVICE_REPLY_OFFSET: u64 = 64;
/// 只读 + 可写映射权（`SegMapR|SegMapW`）。
const RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));
/// 段 cap 的子集：`SegMapR|SegMapW` + `Derive`（`memseg_map` 前置要求段 cap 含 `Derive`）。
const RW_DERIVE: Rights = Rights::from_bits((1 << 0) | (1 << 1) | (1 << 2));
/// 段 cap 的子集：`SegMapR` + `Derive`（交给 init 映射只读）。
const R_DERIVE: Rights = Rights::from_bits((1 << 0) | (1 << 1));

/// 端点自报 id 的消息 tag（高 16 位；与 `prima-runtime::protocol`/内核一致）。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// 端点完成协商的消息 tag。
const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
/// 转发 `prepare` 的消息 tag；payload 低 8 位携带路由到的行为码。
const SERVICE_PREPARE_TAG: u64 = 0x5EC3_0000_0000_0000;
/// 应答客户端的 payload（`"PONG"` ‖ opcode ‖ request_id；与内核 `SERVICE_REPLY_WORD` 一致）。
const SERVICE_REPLY_WORD: u64 = 0x0003_8001_474E_4F50;
/// 未注册名（解析拒绝负例的目标）。
const SERVICE_NAME_UNKNOWN: u64 = pack_service_name(b"ghost");

/// 管理器 → init 的控制消息 tag（`0x5E56` 是存活自报；以下三条互异、且与端点 tag 不相交）。
const SERVICEMGR_WAKE_TAG: u64 = 0x5E57_0000_0000_0000;
const SERVICEMGR_SEG_TAG: u64 = 0x5E58_0000_0000_0000;
const SERVICEMGR_FINISH_TAG: u64 = 0x5E59_0000_0000_0000;

/// 本映像编译进二进制的标记（见证按共享段偏移读回，证明字段来自**本独立映像**）。
const SERVICEMGR_BINARY_MARKER: u64 = 0x5345_5256_4D47_5203;

/// 管理器写回客户端共享段的固定偏移（内核 `user.rs` 侧同款）。
const OFF_SERVICEMGR_ID: u64 = 192;
const OFF_SERVICEMGR_MARKER: u64 = 200;
const OFF_RESOLVED: u64 = 208;
const OFF_UNKNOWN: u64 = 216;
const OFF_DONE_BETA: u64 = 224;
const OFF_REG_A_NAME: u64 = 232;
const OFF_REG_B_NAME: u64 = 240;
const OFF_BETA_SELF: u64 = 248;
const OFF_DONE_ALPHA: u64 = 256;
const OFF_CLIENT_ID: u64 = 264;

/// 注册表条目：端点名 → 行为码、task id、发送凭据。
#[derive(Clone, Copy)]
struct Endpoint {
    id: u64,
    authority: Capability,
    behavior: u64,
}

/// 名字 → 行为码（未注册 → `NONE`）。本片注册表为编译期已知的两项；SPEC-105 改为表驱动。
const fn behavior_for(name: u64) -> u64 {
    if name == SERVICE_NAME_ALPHA {
        SERVICE_BEHAVIOR_ALPHA
    } else if name == SERVICE_NAME_BETA {
        SERVICE_BEHAVIOR_BETA
    } else {
        SERVICE_BEHAVIOR_NONE
    }
}

/// 服务管理器入口。装载器提供 ring3 栈；入口参数由 ABI 保留。
///
/// 第 2 参数（`rsi` = `initial_caps[0]`）是 `init` 移交的**回发凭据**（目标 task 1 + `TaskSend`），
/// 本管理器用它向 init 自报存活、并在收尾把共享段只读副本交回 init。第 5 参数（`r8`）是该凭据
/// 所指向的**对端 task id**（由 `task_create` 运行期指派）。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    self_cap_raw: u64,
    init_cap_raw: u64,
    _initial_cap1_raw: u64,
    _name_raw: u64,
    peer_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    let (Ok(self_cap), Ok(init_cap)) = (
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(init_cap_raw),
    ) else {
        idle();
    };
    let init_id = TaskId::from_raw(peer_raw as u32);
    let self_id = runtime.task_id().map_or(0, |task| task.as_raw() as u64);

    // 自报存活：init 据此见证「管理器已建起且 id 非 init」。
    let _ = runtime.send(
        init_id,
        init_cap,
        SERVICEMGR_HELLO_TAG | (self_id & 0xff),
        &[],
    );

    let mut segment: Option<Capability> = None;
    let mut client: Option<Capability> = None;
    let mut client_id = 0u64;
    let mut registry: [Option<Endpoint>; 2] = [None, None];
    let mut collected = 0usize;
    let mut alpha_forwarded = false;
    let mut alpha_done = false;
    let mut beta_forwarded = false;
    let mut beta_done = false;
    let mut finished = false;

    loop {
        let Ok(message) = runtime.receive() else {
            continue;
        };
        if message.capabilities[1].is_some() {
            // ① 客户端 Hello（唯一带**两条** cap 的消息：段 cap + 客户端自管理 cap）。
            if segment.is_none() {
                let (Some(seg), Some(client_cap)) =
                    (message.capabilities[0], message.capabilities[1])
                else {
                    continue;
                };
                client_id = message.payload & 0xff;
                let _ = runtime.memseg_map(seg, self_cap, 0, USER_SEG_VA, 1, RW);
                let _ =
                    runtime.write_user_bytes(USER_SEG_VA + OFF_CLIENT_ID, &client_id.to_le_bytes());
                let _ = runtime
                    .write_user_bytes(USER_SEG_VA + OFF_SERVICEMGR_ID, &self_id.to_le_bytes());
                let _ = runtime.write_user_bytes(
                    USER_SEG_VA + OFF_SERVICEMGR_MARKER,
                    &SERVICEMGR_BINARY_MARKER.to_le_bytes(),
                );
                segment = Some(seg);
                client = Some(client_cap);
            }
        } else if message.payload >> 48 == SERVICE_ID_TAG >> 48 {
            // ② 端点自报（cap0 = 端点自管理凭据，管理器据此回送转发）。
            let name = (message.payload >> 8) & SERVICE_NAME_MASK;
            let behavior = behavior_for(name);
            let slot = (behavior.wrapping_sub(1)) as usize;
            if let Some(authority) = message.capabilities[0]
                && behavior != SERVICE_BEHAVIOR_NONE
                && slot < 2
                && registry[slot].is_none()
            {
                let id = message.payload & 0xff;
                registry[slot] = Some(Endpoint {
                    id,
                    authority,
                    behavior,
                });
                let name_offset = if behavior == SERVICE_BEHAVIOR_BETA {
                    OFF_REG_B_NAME
                } else {
                    OFF_REG_A_NAME
                };
                let _ = runtime.write_user_bytes(USER_SEG_VA + name_offset, &name.to_le_bytes());
                if behavior == SERVICE_BEHAVIOR_BETA {
                    // 把**管理器收集到的**端点 id 写回，与管理器收到的注册名同源。
                    let _ =
                        runtime.write_user_bytes(USER_SEG_VA + OFF_BETA_SELF, &id.to_le_bytes());
                }
                collected += 1;
            }
        } else if message.payload >> 48 == SERVICE_DONE_TAG >> 48 {
            // ③ 端点完成协商。先转发的是 `ALPHA`，故首个 done 属 `ALPHA`。
            if !alpha_done {
                alpha_done = true;
                let _ = runtime
                    .write_user_bytes(USER_SEG_VA + OFF_DONE_ALPHA, &message.payload.to_le_bytes());
            } else {
                beta_done = true;
                let _ = runtime
                    .write_user_bytes(USER_SEG_VA + OFF_DONE_BETA, &message.payload.to_le_bytes());
            }
        }

        let Some(seg) = segment else {
            continue;
        };

        // ④ 首次转发：`ALPHA` 行为（既有 `service_negotiated` 见证的产出方）。
        if collected == 2 && !alpha_forwarded {
            if let Some(alpha) = registry[0]
                && let Ok(copy) = runtime.cap_derive(seg, RW_DERIVE)
            {
                forward(&runtime, &alpha, copy);
            }
            alpha_forwarded = true;
        }

        // ⑤ 读客户端写入共享段的**请求名**并按名解析，再二次转发到解析到的行为。
        if alpha_done && !beta_forwarded {
            let requested = runtime
                .read_user_u64(USER_SEG_VA + SERVICE_NAME_OFFSET)
                .unwrap_or(0);
            let resolved = behavior_for(requested);
            let _ = runtime.write_user_bytes(USER_SEG_VA + OFF_RESOLVED, &resolved.to_le_bytes());
            let _ = runtime.write_user_bytes(
                USER_SEG_VA + OFF_UNKNOWN,
                &behavior_for(SERVICE_NAME_UNKNOWN).to_le_bytes(),
            );
            let slot = resolved.wrapping_sub(1) as usize;
            if resolved != SERVICE_BEHAVIOR_NONE
                && slot < 2
                && let Some(endpoint) = registry[slot]
                && let Ok(copy) = runtime.cap_derive(seg, RW_DERIVE)
            {
                forward(&runtime, &endpoint, copy);
            } else {
                // 未注册名（`NONE`）被拒绝、不转发：无第二个 done，直接允许收尾。
                beta_done = true;
            }
            beta_forwarded = true;
        }

        // ⑥ 两轮转发都完成：应答客户端（`PONG`），并把共享段只读副本交回 init。
        if beta_done
            && !finished
            && let Some(client_cap) = client
        {
            let _ = runtime.send(
                TaskId::from_raw(client_id as u32),
                client_cap,
                SERVICE_REPLY_WORD,
                &[],
            );
            // 等客户端把应答写回共享段（offset 64）后再收尾——否则 init 可能先 `int3`，内核见证
            // 会读到未写入的 0。客户端被 `PONG` 直投唤醒后必然写入。
            loop {
                let _ = runtime.yield_task();
                if runtime
                    .read_user_u64(USER_SEG_VA + SERVICE_REPLY_OFFSET)
                    .unwrap_or(0)
                    != 0
                {
                    break;
                }
            }
            // 三段**连发**（中间不让出）：唤醒先于带 cap 消息，宿主的 `BlockedRecv` 约束不触发。
            let _ = runtime.send(init_id, init_cap, SERVICEMGR_WAKE_TAG, &[]);
            if let Ok(read_only) = runtime.cap_derive(seg, R_DERIVE) {
                let _ = runtime.send(init_id, init_cap, SERVICEMGR_SEG_TAG, &[read_only]);
            }
            let _ = runtime.send(init_id, init_cap, SERVICEMGR_FINISH_TAG, &[]);
            finished = true;
        }
    }
}

/// 转发一条 `prepare(behavior)` 无 cap 唤醒 + 一条携带共享段 cap 的消息给端点。
///
/// 两条**连续**发送（中间不让出）：接收者若阻塞在 `recv`，唤醒消息先解除阻塞，故随后带 cap 的
/// 消息不会被内核以「向 `BlockedRecv` 投递带 cap 消息」拒绝（`syscall.rs::send_call`）。
fn forward(runtime: &Runtime<impl SyscallTransport>, endpoint: &Endpoint, segment: Capability) {
    let receiver = TaskId::from_raw(endpoint.id as u32);
    let _ = runtime.send(
        receiver,
        endpoint.authority,
        SERVICE_PREPARE_TAG | endpoint.behavior,
        &[],
    );
    let _ = runtime.send(receiver, endpoint.authority, 0, &[segment]);
}

fn idle() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    idle()
}
