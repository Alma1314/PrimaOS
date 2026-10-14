//! SPEC-106（S4-D3）：三个**基础服务端点**（日志 / Namespace / 基础存储）的共用骨架。
//!
//! 三者是**各自独立的二进制**（`prima-log`/`prima-ns`/`prima-store`），仅 `NAME`/`KIND`/
//! `MARKER` 三个编译期参数相异——经 `#[path] mod service` 复用本骨架，避免三份近似副本。
//! 每个服务由 ring3 [`prima-servicemgr`](../../servicemgr.rs) 按服务表建立（入口由管理器自解
//! ELF 头得到），启动时向管理器自报：注册名 + 自管理凭据（供管理器回送路由）、以及**编译进自身
//! 二进制**的 40 位存在标记（证明三个服务各由不同程序运行）。
//!
//! 收到管理器的**请求**（[`SERVICE_SERVE_TAG`]）后，各服务按 `op` 语义处理：`ADVANCE` 把 `arg`
//! 并入**自身内部状态**，`QUERY` 只读回当前状态。应答带 `kind`（相异 ⇒ 路由到了正确的服务），
//! 且携带的是**状态**而非入参的回显——纯回声端点无法通过定序「推进/推进/查询」的判决。
//!
//! 服务的最小权限：初始只持两条 cap——服务管理器从自身派生的 `TaskSend`（`initial_cap0`，指向
//! 管理器）与非零角色令牌（`initial_cap1`，由代码段派生）。无段/无 Namespace 权限。

use prima_runtime::{
    Capability, Rights, Runtime, TaskId,
    protocol::{
        SERVICE_BEHAVIOR_LOG, SERVICE_BEHAVIOR_STORE, SERVICE_MARKER_MASK, SERVICE_NAME_MASK,
        SERVICE_SERVE_OP_ADVANCE, SERVICE_SERVE_OP_SHIFT, SERVICE_SERVE_TAG, SERVICE_WAKE_TAG,
        pack_service_marker, pack_service_reply,
    },
};

/// 端点/服务自报的消息 tag（高 16 位；与 `prima-runtime::protocol`/内核/`beta.rs` 同款）。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// 派生 `TaskSend` 用（服务→管理器）。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);
/// 引导任务（init）的 task id。对端是它时**不**发无 cap 唤醒。
const BOOTSTRAP_TASK: u32 = 1;

/// 基础服务主体：自报注册名/存在标记，随后应答路由探针。
pub fn run(
    name: u64,
    kind: u64,
    marker: u64,
    self_cap_raw: u64,
    send_cap_raw: u64,
    peer_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    let peer = TaskId::from_raw(peer_raw as u32);
    let (Ok(self_cap), Ok(send_cap)) = (
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(send_cap_raw),
    ) else {
        idle();
    };
    if let Ok(id) = runtime.task_id()
        && let Ok(authority) = runtime.cap_derive(self_cap, TASK_SEND_RIGHTS)
    {
        // 与端点同款：先发无 cap 唤醒（管理器常态阻塞在 `recv`，内核拒绝向 `BlockedRecv` 投带 cap
        // 消息），再带自管理凭据自报注册名 + id。
        if peer.as_raw() != BOOTSTRAP_TASK {
            let _ = runtime.send(peer, send_cap, SERVICE_WAKE_TAG, &[]);
        }
        let _ = runtime.send(
            peer,
            send_cap,
            SERVICE_ID_TAG | ((name & SERVICE_NAME_MASK) << 8) | (id.as_raw() as u64 & 0xff),
            &[authority],
        );
        // 存在标记：`kind` 归槽、`marker` 逐二进制相异。
        let _ = runtime.send(
            peer,
            send_cap,
            pack_service_marker(kind, marker & SERVICE_MARKER_MASK),
            &[],
        );
    }
    // SPEC-110：服务内部状态——`op=ADVANCE` 请求按 `kind` 语义并入 `arg`，`op=QUERY`（或未知 op）
    // 只读回。三服务的转移语义各异，故同一请求序列得到三个互异的状态值。
    let mut state: u64 = 0;
    loop {
        let Ok(message) = runtime.receive() else {
            continue;
        };
        if message.payload >> 48 == SERVICE_SERVE_TAG >> 48 {
            let op = (message.payload >> SERVICE_SERVE_OP_SHIFT) & 0xff;
            if op == SERVICE_SERVE_OP_ADVANCE {
                state = advance(kind, state, message.payload & 0xffff_ffff);
            }
            let _ = runtime.send(peer, send_cap, pack_service_reply(kind, state), &[]);
        }
    }
}

/// SPEC-110：各基础服务的状态转移——**语义各异**（同一请求序列 → 三个互异的状态值）。
///
/// - 日志：`state += arg`（累计写入字节数）
/// - 存储：`state = (state << 8) | (arg & 0xff)`（追加低字节，像移位写入）
/// - Namespace：`state = (state << 4) ^ arg`（滚动指纹）
fn advance(kind: u64, state: u64, arg: u64) -> u64 {
    match kind {
        SERVICE_BEHAVIOR_LOG => state.wrapping_add(arg),
        SERVICE_BEHAVIOR_STORE => (state << 8) | (arg & 0xff),
        _ => (state << 4) ^ arg,
    }
}

fn idle() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    idle()
}
