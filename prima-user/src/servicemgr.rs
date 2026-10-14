//! SPEC-104/105/106：**ring3 服务管理器**——运行期注册表、按名解析、请求路由/转发，以及
//! **表驱动的端点创建**的宿主。
//!
//! 决策见 `docs/superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md`：把服务发现、
//! 路由与端点创建从 `init` 的手写 x86 机器码迁到本独立 ring3 任务。本映像**不含任何服务名字面量
//! 之外的对端/端点 task id**——端点与客户端都以本管理器为对端（对端 id 由创建时经入口 `r8`
//! 运行期指派），本管理器按自报消息里的名字建注册表、按客户端写入共享段的请求名解析、并把
//! `prepare` 与共享段 cap 转发给解析到的端点。
//!
//! **SPEC-105**：端点不再由 `init` 的手写汇编创建。`init` 只把建端点所需的 cap 移交本管理器
//! （父 Namespace + 每服务的代码段/栈段），本管理器按 [`SERVICE_TABLE`] 表驱动地
//! `task_create` + 映射栈；入口地址由本管理器**自行解析代码段 ELF 头的 `e_entry`** 得到。
//!
//! **SPEC-106（S4-D3）**：服务表从 2 项泛化为 [`SERVICES`] 项，新增三个**基础服务**端点
//! （日志 / Namespace / 基础存储，各为独立二进制 `prima-log`/`prima-ns`/`prima-store`）。三者与
//! `ALPHA`/`BETA` 走同一条表驱动创建路径，但不参与 wire 协商：各自向本管理器自报**存在标记**
//! （`kind` 归槽 + 每二进制相异的 40 位值），随后本管理器向三者各发一条**路由探针**并收回应答。
//! 注册名、存在标记与探针应答写回共享段，构成「三服务由本管理器按表建立、且请求路由到了正确的
//! 服务二进制」的逐字段见证。
//!
//! 与本映像一起构成判决性见证的，是本管理器写回**客户端共享段**固定偏移的自证字段（自身 task
//! id + 二进制标记）与解析结果——内核见证按共享段偏移读回，故「注册表/解析由独立 ring3 映像
//! 产出」是逐字节可核验的，而非仅凭 init 的回执。
//!
//! 消息处理是**顺序无关的事件循环**：客户端的 Hello 与各端点自报、done、标记与探针应答、以及
//! `init` 的建端点移交可按任意次序到达，状态齐全时才推进下一阶段。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, SyscallTransport, TaskId,
    protocol::{
        ELF_ENTRY_OFFSET, ENDPOINT_CODE_PROBE_VA, ENDPOINT_CODE_VA, ENDPOINT_STACK_VA,
        SERVICE_BEHAVIOR_ALPHA, SERVICE_BEHAVIOR_BETA, SERVICE_BEHAVIOR_LOG, SERVICE_BEHAVIOR_NONE,
        SERVICE_BEHAVIOR_NS, SERVICE_BEHAVIOR_STORE, SERVICE_IMAGE_DIR_HEADER,
        SERVICE_IMAGE_ENTRY_LEN, SERVICE_IMAGE_MAGIC, SERVICE_MARKER_KIND_SHIFT,
        SERVICE_MARKER_MASK, SERVICE_MARKER_TAG, SERVICE_NAME_ALPHA, SERVICE_NAME_BETA,
        SERVICE_NAME_LOG, SERVICE_NAME_MASK, SERVICE_NAME_NS, SERVICE_NAME_OFFSET,
        SERVICE_NAME_STORE, SERVICE_SERVE_ARG_A, SERVICE_SERVE_ARG_B, SERVICE_SERVE_OP_ADVANCE,
        SERVICE_SERVE_OP_QUERY, SERVICE_SERVE_PROBES, SERVICE_SERVE_TAG, SERVICE_SETUP_TAG,
        SERVICEMGR_HELLO_TAG, pack_service_name, pack_service_request,
    },
    task::TaskSpec,
};

/// 客户端把共享段映射到的 VA（与本进程自身根内的同一物理帧映射目标；与 `prima-user` 一致）。
const USER_SEG_VA: u64 = 0x0000_0A00_0000_7000;
/// 客户端把 `PONG` 的 payload 写回共享段内的偏移（与 `prima-user`/内核一致）。
const SERVICE_REPLY_OFFSET: u64 = 64;
/// 端点用户栈的页大小（`stack_top = ENDPOINT_STACK_VA + 0x1000`）。
const FRAME_SIZE: u64 = 0x1000;
/// 只读 + 可写映射权（`SegMapR|SegMapW`）。
const RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));
/// 段 cap 的子集：`SegMapR|SegMapW` + `Derive`（`memseg_map` 前置要求段 cap 含 `Derive`）。
const RW_DERIVE: Rights = Rights::from_bits((1 << 0) | (1 << 1) | (1 << 2));
/// 段 cap 的子集：`SegMapR` + `Derive`（交给 init 映射只读）。
const R_DERIVE: Rights = Rights::from_bits((1 << 0) | (1 << 1));
/// 只读映射权（`SegMapR`）——自解 ELF 入口与 endpoint 令牌用。
const R_ONLY: Rights = Rights::from_bits(1 << 1);
/// 端点初始帧 `initial_cap0` 的权利（仅 `TaskSend`；指向本管理器）。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);
/// 新端点结构性根的 Namespace 权利（仅 `NsResolve`）。
const NS_RESOLVE_RIGHTS: Rights = Rights::from_bits(1 << 8);

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
/// SPEC-105：见证管理器**确实创建了**端点及其收到的建端点参数。
const OFF_MGR_CREATED: u64 = 272;
const OFF_MGR_TOKEN: u64 = 280;
const OFF_MGR_MANAGER: u64 = 288;
/// SPEC-105：两个端点自解出的 ELF 入口（load_base 相对值），证明入口来自**本管理器的解析**。
const OFF_MGR_ENTRY_A: u64 = 304;
const OFF_MGR_ENTRY_B: u64 = 312;
/// SPEC-106：三个基础服务的 (id, 存在标记, 探针应答)，各占 8 B；以及已应答的基础服务数。
const OFF_LOG_ID: u64 = 320;
const OFF_LOG_MARK: u64 = 328;
const OFF_NS_ID: u64 = 336;
const OFF_NS_MARK: u64 = 344;
const OFF_STORE_ID: u64 = 352;
const OFF_STORE_MARK: u64 = 360;
const OFF_SERVICES: u64 = 368;
/// SPEC-110：每个基础服务的**定序状态见证**占 3 槽（推进 A / 推进 B / 查询），共 24 B。
const OFF_LOG_REPLY: u64 = 376;
const OFF_NS_REPLY: u64 = 400;
const OFF_STORE_REPLY: u64 = 424;

/// 服务表条目数（`ALPHA`/`BETA` + 日志/Namespace/存储三个基础服务）。
const SERVICES: usize = 5;
/// 三个基础服务的表下标起点（`SERVICE_BEHAVIOR_LOG` = 3 ⇒ 下标 2）。
const BASE_FIRST: usize = 2;

/// 注册表条目：端点名 → 行为码、task id、发送凭据。
#[derive(Clone, Copy)]
struct Endpoint {
    id: u64,
    authority: Capability,
    behavior: u64,
}

/// SPEC-105/108：待创建的服务（`init` 移交的**代码段** cap + 运行期参数）。
///
/// SPEC-108：**栈段**不再由 `init` 移交——[`create_service`] 用 `SYS_MEMSEG_CREATE` 自建。
#[derive(Clone, Copy)]
struct Pending {
    code: Capability,
    offset_pages: u64,
    pages: u64,
    name: u64,
}

/// SPEC-105/106：**服务表**——名字 → 行为码。加一个服务 = 表里一行 + 一个二进制 + init 移交一对
/// cap（「init 零改动」尚未做到，见报告登记）。
const SERVICE_TABLE: [(u64, u64); SERVICES] = [
    (SERVICE_NAME_ALPHA, SERVICE_BEHAVIOR_ALPHA),
    (SERVICE_NAME_BETA, SERVICE_BEHAVIOR_BETA),
    (SERVICE_NAME_LOG, SERVICE_BEHAVIOR_LOG),
    (SERVICE_NAME_NS, SERVICE_BEHAVIOR_NS),
    (SERVICE_NAME_STORE, SERVICE_BEHAVIOR_STORE),
];

/// 名字 → 行为码（未注册 → `NONE`）。表驱动：SPEC-105 起扫描 [`SERVICE_TABLE`]。
fn behavior_for(name: u64) -> u64 {
    let mut index = 0;
    while index < SERVICES {
        if SERVICE_TABLE[index].0 == name {
            return SERVICE_TABLE[index].1;
        }
        index += 1;
    }
    SERVICE_BEHAVIOR_NONE
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
    image_cap_raw: u64,
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
    // SPEC-109：`initial_cap1` 是内核打包的**单一服务映像段**（零 IPC 移交）。
    let Ok(image_cap) = Capability::from_raw(image_cap_raw) else {
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
    let mut registry: [Option<Endpoint>; SERVICES] = [None; SERVICES];
    let mut alpha_forwarded = false;
    let mut alpha_done = false;
    let mut beta_forwarded = false;
    let mut beta_done = false;
    let mut finished = false;
    // SPEC-105/106：建端点状态。
    let mut ns: Option<Capability> = None;
    let mut pending: [Option<Pending>; SERVICES] = [None; SERVICES];
    // SPEC-109：从映像段首页目录一次性读出全部服务的（名字 → 段内页偏移/页数）。
    let _ = read_image_directory(&runtime, &self_cap, image_cap, &mut pending);
    let mut created = 0u64;
    let mut created_slot: [bool; SERVICES] = [false; SERVICES];
    // SPEC-106：三个基础服务的存在标记与路由探针应答。
    let mut marker: [u64; SERVICES] = [0; SERVICES];
    let mut probe_sent = false;
    // SPEC-110：每个基础服务已收到的应答条数（0..3）与**已答完 3 条**的服务数。
    let mut reply_count: [u64; SERVICES] = [0; SERVICES];
    let mut served = 0u64;

    loop {
        let Ok(message) = runtime.receive() else {
            continue;
        };
        if message.payload >> 48 == SERVICE_SETUP_TAG >> 48 {
            // ⓪' SPEC-105：init 移交父 Namespace（单 cap，payload 仅 tag）。
            if let Some(cap0) = message.capabilities[0]
                && ns.is_none()
            {
                ns = Some(cap0);
            }
        } else if message.capabilities[1].is_some() {
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
            // ② 端点/服务自报（cap0 = 自管理凭据，管理器据此回送转发/路由探针）。
            let name = (message.payload >> 8) & SERVICE_NAME_MASK;
            let behavior = behavior_for(name);
            let slot = behavior.wrapping_sub(1) as usize;
            if let Some(authority) = message.capabilities[0]
                && behavior != SERVICE_BEHAVIOR_NONE
                && slot < SERVICES
                && registry[slot].is_none()
            {
                let id = message.payload & 0xff;
                registry[slot] = Some(Endpoint {
                    id,
                    authority,
                    behavior,
                });
                if let Some(seg) = segment {
                    if behavior == SERVICE_BEHAVIOR_ALPHA {
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_REG_A_NAME, &name.to_le_bytes());
                    } else if behavior == SERVICE_BEHAVIOR_BETA {
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_REG_B_NAME, &name.to_le_bytes());
                        // 把**管理器收集到的**端点 id 写回，与管理器收到的注册名同源。
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_BETA_SELF, &id.to_le_bytes());
                    } else {
                        let _ = runtime.write_user_bytes(
                            USER_SEG_VA + base_id_offset(slot),
                            &id.to_le_bytes(),
                        );
                    }
                    let _ = seg;
                }
            }
        } else if message.payload >> 48 == SERVICE_DONE_TAG >> 48 {
            // ③ 端点完成协商。先转发的是 `ALPHA`，故首个 done 属 `ALPHA`。
            if !alpha_done {
                alpha_done = true;
                if segment.is_some() {
                    let _ = runtime.write_user_bytes(
                        USER_SEG_VA + OFF_DONE_ALPHA,
                        &message.payload.to_le_bytes(),
                    );
                }
            } else {
                beta_done = true;
                if segment.is_some() {
                    let _ = runtime.write_user_bytes(
                        USER_SEG_VA + OFF_DONE_BETA,
                        &message.payload.to_le_bytes(),
                    );
                }
            }
        } else if message.payload >> 48 == SERVICE_MARKER_TAG >> 48 {
            // ③' SPEC-106：基础服务自报存在标记（kind 归槽 + 每二进制相异的 marker）。
            let kind = (message.payload >> SERVICE_MARKER_KIND_SHIFT) & 0xff;
            let slot = kind.wrapping_sub(1) as usize;
            if (BASE_FIRST..SERVICES).contains(&slot) && marker[slot] == 0 {
                marker[slot] = message.payload & SERVICE_MARKER_MASK;
                if segment.is_some() {
                    let _ = runtime.write_user_bytes(
                        USER_SEG_VA + base_mark_offset(slot),
                        &marker[slot].to_le_bytes(),
                    );
                }
            }
        } else if message.payload >> 48 == SERVICE_SERVE_TAG >> 48 {
            // ③'' SPEC-106/110：基础服务的**定序状态应答**（kind 归槽，按到达顺序填该服务的第
            // 0/1/2 槽——同一发送者到同一服务保序，故推进 A、推进 B、查询的应答依序落位）。
            let kind = (message.payload >> 32) & 0xff;
            let slot = kind.wrapping_sub(1) as usize;
            if (BASE_FIRST..SERVICES).contains(&slot) && reply_count[slot] < SERVICE_SERVE_PROBES {
                let index = reply_count[slot];
                reply_count[slot] += 1;
                if segment.is_some() {
                    let _ = runtime.write_user_bytes(
                        USER_SEG_VA + base_reply_offset(slot) + index * 8,
                        &message.payload.to_le_bytes(),
                    );
                    // 该服务三条定序应答齐 ⇒ 服务数 +1（`services` 语义不变）。
                    if reply_count[slot] == SERVICE_SERVE_PROBES {
                        served += 1;
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_SERVICES, &served.to_le_bytes());
                    }
                }
            }
        }

        // ⓪' SPEC-105/106：cap、参数与共享段齐全即创建端点（表驱动）——建端点证据写回共享段，故
        // 须待客户端 Hello 把段映射进本根之后。
        if let (Some(ns_cap), Some(_)) = (ns, segment) {
            let mut slot = 0;
            while slot < SERVICES {
                if !created_slot[slot]
                    && let Some(spec) = pending[slot]
                    && create_service(&runtime, &self_cap, ns_cap, &spec, slot, &mut created_slot)
                {
                    created += 1;
                    let _ = runtime
                        .write_user_bytes(USER_SEG_VA + OFF_MGR_CREATED, &created.to_le_bytes());
                }
                slot += 1;
            }
        }

        let Some(seg) = segment else {
            continue;
        };

        // ④ 首次转发：`ALPHA` 行为（既有 `service_negotiated` 见证的产出方）。以注册表就绪为准，
        // 与基础服务的注册互不耦合。
        if registry[0].is_some() && registry[1].is_some() && !alpha_forwarded {
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
                && slot < SERVICES
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

        // ⑤' SPEC-106/110：三个基础服务都已注册即各发**定序三条**请求（推进 A、推进 B、查询，
        // 只发一次）。同一发送者保序，故服务按推进→推进→查询处理，应答依序落位各自的状态槽。
        if !probe_sent
            && registry[BASE_FIRST].is_some()
            && registry[BASE_FIRST + 1].is_some()
            && registry[BASE_FIRST + 2].is_some()
        {
            let mut slot = BASE_FIRST;
            while slot < SERVICES {
                if let Some(endpoint) = registry[slot] {
                    let task = TaskId::from_raw(endpoint.id as u32);
                    let _ = runtime.send(
                        task,
                        endpoint.authority,
                        pack_service_request(SERVICE_SERVE_OP_ADVANCE, SERVICE_SERVE_ARG_A),
                        &[],
                    );
                    let _ = runtime.send(
                        task,
                        endpoint.authority,
                        pack_service_request(SERVICE_SERVE_OP_ADVANCE, SERVICE_SERVE_ARG_B),
                        &[],
                    );
                    let _ = runtime.send(
                        task,
                        endpoint.authority,
                        pack_service_request(SERVICE_SERVE_OP_QUERY, 0),
                        &[],
                    );
                }
                slot += 1;
            }
            probe_sent = true;
        }

        // ⑥ 两轮转发及三个基础服务探针都完成：应答客户端（`PONG`），并把共享段只读副本交回 init。
        if beta_done
            && served == (SERVICES - BASE_FIRST) as u64
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

/// SPEC-109：映射映像段**首页目录**（只读）读出服务表——名字 → 段内页偏移/页数。
///
/// 目录布局（little-endian u64）：`magic | count | [name | offset_pages | page_count] × count`。
/// 读毕即卸载探测映射。目录非法或无匹配时静默跳过（服务缺失由后续见证暴露）。
fn read_image_directory(
    runtime: &Runtime<impl SyscallTransport>,
    self_cap: &Capability,
    image: Capability,
    pending: &mut [Option<Pending>; SERVICES],
) -> Result<(), ()> {
    runtime
        .memseg_map(image, *self_cap, 0, ENDPOINT_CODE_PROBE_VA, 1, R_ONLY)
        .map_err(|_| ())?;
    let magic = runtime.read_user_u64(ENDPOINT_CODE_PROBE_VA).unwrap_or(0);
    let count = runtime
        .read_user_u64(ENDPOINT_CODE_PROBE_VA + 8)
        .unwrap_or(0);
    if magic == SERVICE_IMAGE_MAGIC {
        let mut index = 0u64;
        while index < count && index < SERVICES as u64 {
            let at =
                ENDPOINT_CODE_PROBE_VA + SERVICE_IMAGE_DIR_HEADER + index * SERVICE_IMAGE_ENTRY_LEN;
            let name = runtime.read_user_u64(at).unwrap_or(0);
            let offset_pages = runtime.read_user_u64(at + 8).unwrap_or(0);
            let pages = runtime.read_user_u64(at + 16).unwrap_or(0);
            if pages != 0
                && let Some(slot) = table_slot(name)
            {
                pending[slot] = Some(Pending {
                    code: image,
                    offset_pages,
                    pages,
                    name,
                });
            }
            index += 1;
        }
    }
    let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, 1);
    Ok(())
}

/// SPEC-105/106：名字对应的服务表下标（未注册 → `None`）。
fn table_slot(name: u64) -> Option<usize> {
    let mut index = 0;
    while index < SERVICES {
        if SERVICE_TABLE[index].0 == name {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// SPEC-106：基础服务（下标 ≥ [`BASE_FIRST`]）在共享段内的 id 见证偏移。
fn base_id_offset(slot: usize) -> u64 {
    [OFF_LOG_ID, OFF_NS_ID, OFF_STORE_ID][slot - BASE_FIRST]
}

/// SPEC-106：基础服务在共享段内的存在标记见证偏移。
fn base_mark_offset(slot: usize) -> u64 {
    [OFF_LOG_MARK, OFF_NS_MARK, OFF_STORE_MARK][slot - BASE_FIRST]
}

/// SPEC-106/110：基础服务在共享段内的**定序状态见证起始**偏移（第 `index` 条读 `+ index*8`）。
fn base_reply_offset(slot: usize) -> u64 {
    [OFF_LOG_REPLY, OFF_NS_REPLY, OFF_STORE_REPLY][slot - BASE_FIRST]
}

/// SPEC-105/108：表驱动地创建一个端点——自解 ELF 入口 → **自建栈段** → `task_create` → 映射栈。
///
/// 创建者（本管理器）从自身 `self_cap` 派生指向自己的 `TaskSend` 凭据作 `initial_cap0`（端点据此
/// 自报/发 done 直达本管理器），从代码段派生只读令牌作 `initial_cap1`（非零 = 端点角色）。
///
/// **SPEC-108**：栈段由本管理器 `SYS_MEMSEG_CREATE` **自建**（不再由 `init` 移交）——服务任务的
/// 全部创建（代码段之外的栈）就此收敛到 ring3。`memseg_create` 铸出的根 cap 属主是本管理器，
/// 故随后可 `memseg_map` 进新端点根。
#[allow(clippy::too_many_arguments)]
fn create_service(
    runtime: &Runtime<impl SyscallTransport>,
    self_cap: &Capability,
    ns_cap: Capability,
    spec: &Pending,
    slot: usize,
    created_slot: &mut [bool; SERVICES],
) -> bool {
    created_slot[slot] = true;
    let (Ok(initial0), Ok(initial1)) = (
        runtime.cap_derive(*self_cap, TASK_SEND_RIGHTS),
        runtime.cap_derive(spec.code, R_ONLY),
    ) else {
        return false;
    };
    // 自解 ELF 入口：把代码段只读映射到本管理器根内探测页读 `e_entry`（偏移 24..32，未被
    // 装载器覆写），随后卸载——入口不来自 init 的手写字面量。
    let Ok(()) = runtime.memseg_map(
        spec.code,
        *self_cap,
        spec.offset_pages,
        ENDPOINT_CODE_PROBE_VA,
        spec.pages,
        R_ONLY,
    ) else {
        return false;
    };
    let e_entry = runtime
        .read_user_u64(ENDPOINT_CODE_PROBE_VA + ELF_ENTRY_OFFSET)
        .unwrap_or(u64::MAX);
    let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, spec.pages);
    if e_entry == u64::MAX {
        return false;
    }
    // SPEC-108：自建 1 页栈段（`R|W`）。
    let Ok(stack) = runtime.memseg_create(1, RW) else {
        return false;
    };
    let entry = ENDPOINT_CODE_VA + e_entry;
    let Ok(manager) = runtime.task_create(&TaskSpec {
        code_segment: spec.code,
        code_offset: spec.offset_pages,
        code_pages: spec.pages,
        load_base: ENDPOINT_CODE_VA,
        entry,
        stack_top: ENDPOINT_STACK_VA + FRAME_SIZE,
        parent_namespace: Some(ns_cap),
        root_rights: NS_RESOLVE_RIGHTS,
        initial_capabilities: [Some(initial0), Some(initial1)],
        name: spec.name,
    }) else {
        return false;
    };
    // 独立栈段映射进新端点根（`memseg_map` 的目标根由 `manager` cap 的目标 task 派生）。
    let _ = runtime.memseg_map(stack, manager, 0, ENDPOINT_STACK_VA, 1, RW);
    write_created_witness(runtime, slot, e_entry, initial1, manager);
    true
}

/// SPEC-105：把「管理器确实创建了端点」的证据、收到的建端点参数与**自解出的 ELF 入口**
/// 写回共享段固定偏移（内核见证据此读回）。
fn write_created_witness(
    runtime: &Runtime<impl SyscallTransport>,
    slot: usize,
    entry_offset: u64,
    token: Capability,
    manager: Capability,
) {
    // 入口只记 `ALPHA`(槽 0)/`BETA`(槽 1) 的——基础服务不写该字段，避免覆盖既有见证。
    let entry_offset_field = match slot {
        0 => Some(OFF_MGR_ENTRY_A),
        1 => Some(OFF_MGR_ENTRY_B),
        _ => None,
    };
    if let Some(field) = entry_offset_field {
        let _ = runtime.write_user_bytes(USER_SEG_VA + field, &entry_offset.to_le_bytes());
    }
    // 令牌/管理 cap 只记 **ALPHA**（槽 0）的——内核 `service_negotiated_ready` 讲的是 ALPHA 的
    // 协商，而后建的端点/服务会覆盖同一偏移。
    if slot == 0 {
        let _ =
            runtime.write_user_bytes(USER_SEG_VA + OFF_MGR_TOKEN, &token.as_raw().to_le_bytes());
        let _ = runtime.write_user_bytes(
            USER_SEG_VA + OFF_MGR_MANAGER,
            &manager.as_raw().to_le_bytes(),
        );
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
