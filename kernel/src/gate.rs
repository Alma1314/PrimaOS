//! S6-M1 / M1b：内核门控（Gate）与请求生命周期。
//!
//! 本模块是[协议收口](../docs/superpowers/specs/2026-10-29-stage6-m1-protocol.md)的内核对象层：
//! 一个固定服务者 Task、多个客户端绑定、有界 FIFO 队列、每绑定预留一个请求槽与一个私有等待对象。
//! 客户端 trap 之后由服务者经 `SYS_GATE_*` 读取原始 `number`/6 个参数并答复；内核只搬运不透明
//! `u64`，不比较调用号、不选择 Linux errno（S6-X3）。
//!
//! **本切片的边界**：这里只落地对象、预留、队列、一次性答复与清理事务，并用 host 单测钉住；
//! 尚未分配 73–78 机器 ABI、尚未接管 trap 入口。适配层必须在同一个不可调度区间内调用
//! [`BootstrapState::gate_reply`]（它已写入客户端保存帧的 `rax`），随后才允许调度；
//! `gate_copy_in/out` 属 M1c。
//!
//! 失败关闭：所有门控事务在任何变更前预检审计容量；不足时记录首次诊断并返回
//! [`Error::AuditFull`]，由 [`gate_audit`](super::gate_audit) 的全局停止控制器接管。
//!
//! 表存放在静态 arena，实例索引复用审计区段号（与 `gate_audit` 的诊断单元同一模式），
//! 因此不增加 `BootstrapState` 的 13 KiB 结构体尺寸。
use super::*;
use crate::handle::Handle;
use crate::rights::{Right, Rights};

/// 门控对象上限（模型常量，不是机器 ABI 保证）。
pub const MAX_GATES: usize = 16;
/// 绑定/未决请求槽上限：每个绑定在 attach 时预留一个请求槽。
pub const MAX_BINDINGS: usize = 64;
/// 每个门控可递送队列深度；满后进入 `WaitingCapacity`。
pub const QUEUE_DEPTH: usize = 8;
/// `gate_wait` 输出布局：12 个小端 `u64`，共 96 字节。
pub const OUTPUT_WORDS: usize = 12;
pub const OUTPUT_BYTES: u64 = (OUTPUT_WORDS * 8) as u64;
/// 输出布局版本。
pub const VERSION: u64 = 1;

/// Gate 根权利：`{Derive, GateAttach, GateServe}`。
pub const GATE_ROOT_RIGHTS: Rights =
    Rights::from_rights(&[Right::Derive, Right::GateAttach, Right::GateServe]);
/// 请求根权利：`{Derive, GateServe}`。
pub const REQUEST_ROOT_RIGHTS: Rights = Rights::from_rights(&[Right::Derive, Right::GateServe]);

#[cfg(not(test))]
const GATE_REGIONS: usize = 1;
#[cfg(test)]
const GATE_REGIONS: usize = 512;

static GATES: crate::arena::Arena<GateRecord, MAX_GATES, GATE_REGIONS> = crate::arena::Arena::new();
static BINDINGS: crate::arena::Arena<BindingRecord, MAX_BINDINGS, GATE_REGIONS> =
    crate::arena::Arena::new();
static REQUESTS: crate::arena::Arena<RequestRecord, MAX_BINDINGS, GATE_REGIONS> =
    crate::arena::Arena::new();

/// Per-instance table identity, keyed by the audit region exactly like the
/// fail-stop diagnostic cell. Keeps `BootstrapState` within its size guard.
static TABLES: StaticCell<[Option<Tables>; GATE_REGIONS]> = StaticCell::new([None; GATE_REGIONS]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// 权限/身份不匹配（缺 GateAttach/GateServe、非固定服务者、非持有者）。
    Denied,
    /// 门控/绑定/槽位/等待对象额度不足。
    Capacity,
    /// 目标状态不允许（非 Ready、已有绑定、已有未决 trap）。
    Busy,
    /// 对象已终结或代次不匹配。
    Stale,
    /// 审计耗尽：内核进入全局 fail-stop，不把内部错误返回给客户端。
    AuditFull,
}

/// 请求状态；`Idle` 表示该绑定预留的槽当前空闲。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    WaitingCapacity,
    Queued,
    Delivered,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Replied,
    Aborted,
    Orphaned,
    Cancelled,
}

/// 门控对象。内容在静态 arena；[`Tables`] 只持有区段句柄。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateRecord {
    pub id: u32,
    pub service: u32,
    pub service_cookie: u64,
    pub root_cap: Handle,
    pub live: bool,
    pub bindings: u32,
    pub service_wait: Option<completion::CompletionWait>,
    pub service_ticket: Option<completion::CompletionTicket>,
    pub queue: [u32; QUEUE_DEPTH],
    pub queue_head: u32,
    pub queue_len: u32,
    pub capacity: [u32; MAX_BINDINGS],
    pub capacity_head: u32,
    pub capacity_len: u32,
}

/// 单个客户端的绑定；预留的私有等待对象在客户端销毁前一直归它所有。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingRecord {
    pub task: u32,
    pub cookie: u64,
    pub gate: u32,
    /// 服务授权消失后该客户端 trap 的返回值（`attach` 配置）。
    pub orphan: u64,
    pub slot: u32,
    pub wait: completion::CompletionWait,
    pub active: bool,
}

/// 单个请求；`id` 单调分配、永不复用，槽位在绑定存续期内复用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestRecord {
    pub id: u32,
    pub gate: u32,
    pub binding: u32,
    pub client: u32,
    pub client_cookie: u64,
    pub number: u64,
    pub args: [u64; 6],
    pub flags: u64,
    pub phase: Phase,
    pub terminal: u64,
    pub ticket: Option<completion::CompletionTicket>,
    pub root_cap: Option<Handle>,
}

impl RequestRecord {
    const IDLE: Self = Self {
        id: 0,
        gate: 0,
        binding: 0,
        client: 0,
        client_cookie: 0,
        number: 0,
        args: [0; 6],
        flags: 0,
        phase: Phase::Idle,
        terminal: 0,
        ticket: None,
        root_cap: None,
    };
}

/// 门控表（区段句柄 + 单调 id 游标），存放在静态 `TABLES`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tables {
    gates: crate::arena::ArenaSlice,
    bindings: crate::arena::ArenaSlice,
    requests: crate::arena::ArenaSlice,
    pub next_gate: u32,
    pub next_request: u32,
    pub gate_count: u32,
    pub binding_count: u32,
}

impl Tables {
    pub fn claim() -> Self {
        Self {
            gates: GATES.claim(),
            bindings: BINDINGS.claim(),
            requests: REQUESTS.claim(),
            next_gate: 1,
            next_request: 1,
            gate_count: 0,
            binding_count: 0,
        }
    }

    pub fn find(&self, id: u32) -> Option<GateRecord> {
        if id == 0 {
            return None;
        }
        GATES.get(self.gates, id as usize - 1)
    }

    fn write(&self, gate: GateRecord) {
        GATES.set(self.gates, gate.id as usize - 1, Some(gate));
    }

    pub fn binding(&self, slot: u32) -> Option<BindingRecord> {
        BINDINGS.get(self.bindings, slot as usize)
    }

    fn set_binding(&self, slot: u32, binding: Option<BindingRecord>) {
        BINDINGS.set(self.bindings, slot as usize, binding);
    }

    pub fn slot_request(&self, slot: u32) -> Option<RequestRecord> {
        REQUESTS.get(self.requests, slot as usize)
    }

    fn set_request(&self, slot: u32, request: RequestRecord) {
        REQUESTS.set(self.requests, slot as usize, Some(request));
    }

    fn clear_request(&self, slot: u32) {
        REQUESTS.set(
            self.requests,
            slot as usize,
            Some(RequestRecord {
                binding: slot,
                ..RequestRecord::IDLE
            }),
        );
    }

    /// 按单调请求 id 查找；终结后 id 已清零，旧 id 不再命中。
    pub fn request(&self, id: u32) -> Option<RequestRecord> {
        (0..MAX_BINDINGS as u32).find_map(|slot| {
            self.slot_request(slot)
                .filter(|request| request.phase != Phase::Idle && request.id == id)
        })
    }

    fn request_slot_of(&self, id: u32) -> Option<u32> {
        (0..MAX_BINDINGS as u32).find(|slot| {
            self.slot_request(*slot)
                .is_some_and(|request| request.phase != Phase::Idle && request.id == id)
        })
    }

    fn free_binding_slot(&self) -> Option<u32> {
        (0..MAX_BINDINGS as u32).find(|slot| {
            self.slot_request(*slot)
                .is_none_or(|request| request.phase == Phase::Idle)
                && !self.binding(*slot).is_some_and(|binding| binding.active)
        })
    }

    /// 门控 `gate` 的活动绑定数（与 `GateRecord::bindings` 对账用）。
    fn bound_count(&self, gate: u32) -> u32 {
        (0..MAX_BINDINGS as u32)
            .filter(|slot| {
                self.binding(*slot)
                    .is_some_and(|binding| binding.active && binding.gate == gate)
            })
            .count() as u32
    }
}

/// attach 成功后交给适配层的信息。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachOutcome {
    pub gate: u32,
    pub gate_cap: Handle,
    pub client: u32,
    pub client_cookie: u64,
    pub slot: u32,
}

/// 被门控任务的一次 trap 结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapOutcome {
    /// 该任务未绑定门控，适配层照常查原生分派表。
    NotGated,
    /// 请求已入队/进入容量等待，适配层必须阻塞当前任务。
    Queued { request: u32 },
    /// 服务授权已消失：直接以配置值返回，不查原生分派表。
    Orphaned { value: u64 },
}

/// `gate_wait` 的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitOutcome {
    /// 已出队并安装请求能力；适配层把 `words()` 写进 96 字节输出。
    Delivered {
        request: u32,
        request_cap: Handle,
        client: u32,
        client_cookie: u64,
        number: u64,
        args: [u64; 6],
        flags: u64,
    },
    /// 队列为空：服务者已登记到私有服务等待对象，适配层必须阻塞并让
    /// 该次控制调用返回「重试」（输出保持不变）。
    Blocked,
}

impl WaitOutcome {
    /// 12 个输出字；`Delivered` 之外没有输出。
    pub fn words(self) -> Option<[u64; OUTPUT_WORDS]> {
        match self {
            WaitOutcome::Delivered {
                request_cap,
                client_cookie,
                number,
                args,
                flags,
                ..
            } => Some([
                VERSION,
                OUTPUT_BYTES,
                request_cap.encode(),
                client_cookie,
                number,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
                flags,
            ]),
            WaitOutcome::Blocked => None,
        }
    }
}

/// `gate_reply`/`gate_abort` 的提交结果：`rax` 已在本提交内写入，这里只留证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplyOutcome {
    pub client: u32,
    pub client_cookie: u64,
    pub value: u64,
}

impl BootstrapState {
    // ------------------------------------------------------------------
    // 实例表
    // ------------------------------------------------------------------

    fn table_index(&self) -> usize {
        self.audit.base / MAX_AUDIT_EVENTS
    }

    pub fn gate_tables(&self) -> Tables {
        TABLES
            .with(|cells| cells[self.table_index()])
            .expect("gate tables claimed at boot")
    }

    fn save_gate_tables(&self, tables: Tables) {
        TABLES.with_mut(|cells| cells[self.table_index()] = Some(tables));
    }

    /// 由 `create_state` 在实例建好后调用：领取三段 arena 并登记实例表。
    pub(super) fn claim_gate_tables(&self) {
        let tables = Tables::claim();
        self.save_gate_tables(tables);
    }

    // ------------------------------------------------------------------
    // 查询
    // ------------------------------------------------------------------

    fn gate_binding_slot(&self, tables: &Tables, task: u32) -> Option<u32> {
        (0..MAX_BINDINGS as u32).find(|slot| {
            tables
                .binding(*slot)
                .is_some_and(|binding| binding.active && binding.task == task)
        })
    }

    fn gate_binding_of(&self, tables: &Tables, task: u32) -> Option<BindingRecord> {
        self.gate_binding_slot(tables, task)
            .and_then(|slot| tables.binding(slot))
    }

    /// 该 Task 作为固定服务者拥有的第一个活动门控。
    fn gate_served_by(&self, tables: &Tables, task: u32) -> Option<GateRecord> {
        (1..=tables.gate_count).find_map(|id| {
            tables
                .find(id)
                .filter(|gate| gate.service == task && gate.live)
        })
    }

    fn gate_completion_quota(&self, need: usize) -> bool {
        let used = self
            .wait_objects
            .iter()
            .filter(|object| object.completion_owner.is_some())
            .count();
        used.checked_add(need)
            .is_some_and(|total| total <= MAX_COMPLETION_WAITS)
    }

    /// 客户端必须独占地址空间：单线程 Process（无 Process 的平台 Task 也视为独占）。
    fn gate_client_root_is_exclusive(&self, client: u32) -> bool {
        self.process_of(client)
            .is_none_or(|process| self.process_threads(process) <= 1)
    }

    fn can_mint_gate_capability(&self) -> bool {
        self.slots.normal_free_count() > 0
            && self.positions.count < MAX_CAPABILITY_POSITIONS
            && self.can_record_capability()
    }

    // ------------------------------------------------------------------
    // attach
    // ------------------------------------------------------------------

    /// 把一个 `Ready` 客户端绑定到门控（`gate_cap = 0` 时新建门控并返回新根能力）。
    ///
    /// 前置：调用者持有目标 Task 的 `TaskManageVSpace`；调用者自身不是被门控任务；已存在
    /// 门控时调用者必须是同一固定服务者并持有 `GateAttach`。成功即预留一个请求槽和一个
    /// 私有等待对象（供该客户端 trap 阻塞），失败不留下部分状态。
    pub fn gate_attach(
        &mut self,
        caller: u32,
        task_cap: Handle,
        gate_cap: Handle,
        orphan: u64,
    ) -> Result<AttachOutcome, Error> {
        let caller_cookie = self.task_cookie(caller).ok_or(Error::Denied)?;
        if self.tasks.status_of(caller) != Some(TaskStatus::Running) {
            return Err(Error::Busy);
        }
        let mut tables = self.gate_tables();
        if self.gate_binding_slot(&tables, caller).is_some() {
            return Err(Error::Busy);
        }
        self.authorize(caller, task_cap, Right::TaskManageVSpace)
            .map_err(|_| Error::Denied)?;
        let Some(ObjectRef::Task(client)) = self.target_of(task_cap) else {
            return Err(Error::Denied);
        };
        let client_cookie = self.task_cookie(client).ok_or(Error::Denied)?;
        if self.tasks.status_of(client) != Some(TaskStatus::Ready) {
            return Err(Error::Busy);
        }
        if self.gate_binding_slot(&tables, client).is_some() {
            return Err(Error::Busy);
        }
        if self.gate_served_by(&tables, client).is_some() {
            return Err(Error::Busy);
        }
        if !self.gate_client_root_is_exclusive(client) {
            return Err(Error::Busy);
        }

        let (gate, existing_cap) = if gate_cap.is_zero() {
            if tables.gate_count as usize >= MAX_GATES
                || tables.binding_count as usize >= MAX_BINDINGS
            {
                return Err(Error::Capacity);
            }
            (tables.next_gate, None)
        } else {
            self.authorize(caller, gate_cap, Right::GateAttach)
                .map_err(|_| Error::Denied)?;
            let Some(ObjectRef::Gate(id)) = self.target_of(gate_cap) else {
                return Err(Error::Denied);
            };
            let Some(gate) = tables.find(id) else {
                return Err(Error::Stale);
            };
            if !gate.live {
                return Err(Error::Stale);
            }
            if gate.service != caller || gate.service_cookie != caller_cookie {
                return Err(Error::Denied);
            }
            if gate.bindings as usize >= MAX_BINDINGS
                || tables.binding_count as usize >= MAX_BINDINGS
            {
                return Err(Error::Capacity);
            }
            (id, Some(gate_cap))
        };

        let slot = tables.free_binding_slot().ok_or(Error::Capacity)?;
        let new_gate = existing_cap.is_none();
        // 新门控：GateCreated + CapabilityMinted + 服务等待 Reserved + GateAttached + 客户端等待 Reserved。
        // 已有门控：GateAttached + 客户端等待 Reserved。
        let events = if new_gate { 5 } else { 2 };
        if new_gate && !self.can_mint_gate_capability() {
            return Err(Error::Capacity);
        }
        if !self.gate_completion_quota(if new_gate { 2 } else { 1 }) {
            return Err(Error::Capacity);
        }
        self.preflight_gate_audit(events, caller_cookie, gate_audit::Operation::Attach)
            .map_err(|_| Error::AuditFull)?;

        // 预检已通过：以下每一步都不会因容量/审计失败。
        let mut root_cap = existing_cap.unwrap_or(Handle::ZERO);
        if new_gate {
            let service_wait = self
                .reserve_completion_wait(caller)
                .expect("attach preflighted the service reservation");
            let cap = self
                .mint_root_capability(caller, GATE_ROOT_RIGHTS, ObjectRef::Gate(gate))
                .expect("attach preflighted capability capacity");
            self.audit.append(AuditEvent::CapabilityMinted {
                cap,
                holder: caller,
            });
            root_cap = cap;
            tables.write(GateRecord {
                id: gate,
                service: caller,
                service_cookie: caller_cookie,
                root_cap: cap,
                live: true,
                bindings: 0,
                service_wait: Some(service_wait),
                service_ticket: None,
                queue: [0; QUEUE_DEPTH],
                queue_head: 0,
                queue_len: 0,
                capacity: [0; MAX_BINDINGS],
                capacity_head: 0,
                capacity_len: 0,
            });
            tables.next_gate += 1;
            tables.gate_count += 1;
            self.audit.append(AuditEvent::GateCreated {
                gate,
                service: caller,
                service_cookie: caller_cookie,
            });
        }
        let wait = self
            .reserve_completion_wait(client)
            .expect("attach preflighted the client reservation");
        tables.set_binding(
            slot,
            Some(BindingRecord {
                task: client,
                cookie: client_cookie,
                gate,
                orphan,
                slot,
                wait,
                active: true,
            }),
        );
        tables.clear_request(slot);
        tables.binding_count += 1;
        let mut record = tables.find(gate).expect("gate inserted");
        record.bindings += 1;
        tables.write(record);
        self.save_gate_tables(tables);
        self.audit.append(AuditEvent::GateAttached {
            gate,
            client,
            client_cookie,
            orphan,
        });
        Ok(AttachOutcome {
            gate,
            gate_cap: root_cap,
            client,
            client_cookie,
            slot,
        })
    }

    // ------------------------------------------------------------------
    // trap / wait
    // ------------------------------------------------------------------

    /// 被门控任务的一次 `syscall`/`int 0x80` trap：入队（或容量等待）并登记客户端的
    /// 私有等待。适配层在 `Queued` 时必须阻塞当前任务；调用方已保存返回帧。
    pub fn gate_trap(
        &mut self,
        task: u32,
        number: u64,
        args: [u64; 6],
        flags: u64,
    ) -> Result<TrapOutcome, Error> {
        let mut tables = self.gate_tables();
        let Some(slot) = self.gate_binding_slot(&tables, task) else {
            return Ok(TrapOutcome::NotGated);
        };
        let binding = tables.binding(slot).expect("binding slot");
        if self.task_cookie(task) != Some(binding.cookie) {
            return Err(Error::Stale);
        }
        let Some(gate) = tables.find(binding.gate) else {
            return Err(Error::Stale);
        };
        if !gate.live {
            self.preflight_gate_audit(1, binding.cookie, gate_audit::Operation::Trap)
                .map_err(|_| Error::AuditFull)?;
            self.audit.append(AuditEvent::GateTerminated {
                gate: gate.id,
                request: 0,
                value: binding.orphan,
                reason: Reason::Orphaned,
            });
            return Ok(TrapOutcome::Orphaned {
                value: binding.orphan,
            });
        }
        let Some(client) = self.tasks.find(task) else {
            return Err(Error::Stale);
        };
        if client.status != TaskStatus::Running
            || client.wait_request.is_some()
            || client.pending_wait_result.is_some()
            || client.pending_exc.is_some()
        {
            return Err(Error::Busy);
        }
        let Some(request) = tables.slot_request(slot) else {
            return Err(Error::Stale);
        };
        if request.phase != Phase::Idle {
            return Err(Error::Busy);
        }
        let wake_service = gate.service_ticket.is_some();
        // GateTrapped + 客户端 Blocked(+迁移) + 唤醒服务者时 Completed(+迁移)。
        let events = 3 + if wake_service { 2 } else { 0 };
        self.preflight_gate_audit(events, binding.cookie, gate_audit::Operation::Trap)
            .map_err(|_| Error::AuditFull)?;

        let id = tables.next_request;
        let next = id.checked_add(1).ok_or(Error::Capacity)?;
        tables.next_request = next;
        let mut record = request;
        record.id = id;
        record.gate = gate.id;
        record.binding = slot;
        record.client = task;
        record.client_cookie = binding.cookie;
        record.number = number;
        record.args = args;
        record.flags = flags;
        record.terminal = 0;
        record.root_cap = None;
        let mut gate = gate;
        if gate.queue_len as usize >= QUEUE_DEPTH {
            record.phase = Phase::WaitingCapacity;
            gate.capacity[(gate.capacity_head + gate.capacity_len) as usize % MAX_BINDINGS] = id;
            gate.capacity_len += 1;
        } else {
            record.phase = Phase::Queued;
            gate.queue[(gate.queue_head + gate.queue_len) as usize % QUEUE_DEPTH] = id;
            gate.queue_len += 1;
        }
        tables.write(gate);
        tables.set_request(slot, record);
        self.save_gate_tables(tables);
        self.audit.append(AuditEvent::GateTrapped {
            gate: gate.id,
            request: id,
            client_cookie: binding.cookie,
            number,
            args,
            flags,
        });
        let ticket = self
            .block_completion_wait(binding.wait)
            .expect("attach reserved the client wait");
        let tables = self.gate_tables();
        if let Some(mut record) = tables.slot_request(slot) {
            record.ticket = Some(ticket);
            tables.set_request(slot, record);
        }
        if let Some(service_ticket) = gate.service_ticket {
            self.complete_completion_wait(service_ticket)
                .expect("preflighted the service wake");
            gate.service_ticket = None;
            tables.write(gate);
        }
        self.save_gate_tables(tables);
        Ok(TrapOutcome::Queued { request: id })
    }

    /// 服务者取下一个请求：队列非空则出队、安装请求能力并返回输出字；否则登记到该门控的
    /// 私有服务等待对象（不可警报、无期限）。
    ///
    /// 适配层必须在调用前验证 96 字节输出范围（含 COW 预备）并在返回 `Delivered` 后立即写入；
    /// 返回 `Blocked` 时丢弃该计划、让控制调用以「重试」返回。孤儿化且队列为空返回
    /// [`Error::Stale`]，输出保持不变。
    pub fn gate_wait(&mut self, service: u32, gate_cap: Handle) -> Result<WaitOutcome, Error> {
        if self.slots.check(gate_cap) != SlotState::Live {
            return Err(Error::Stale);
        }
        let service_cookie = self.task_cookie(service).ok_or(Error::Denied)?;
        self.authorize(service, gate_cap, Right::GateServe)
            .map_err(|_| Error::Denied)?;
        let Some(ObjectRef::Gate(gate_id)) = self.target_of(gate_cap) else {
            return Err(Error::Denied);
        };
        let tables = self.gate_tables();
        let Some(gate) = tables.find(gate_id) else {
            return Err(Error::Stale);
        };
        if gate.service != service || gate.service_cookie != service_cookie {
            return Err(Error::Denied);
        }
        if !gate.live {
            return Err(Error::Stale);
        }
        if gate.queue_len == 0 {
            self.preflight_gate_audit(2, service_cookie, gate_audit::Operation::Block)
                .map_err(|_| Error::AuditFull)?;
            let wait = gate.service_wait.ok_or(Error::Stale)?;
            let ticket = self
                .block_completion_wait(wait)
                .expect("gate reserved its service wait");
            let mut gate = gate;
            gate.service_ticket = Some(ticket);
            tables.write(gate);
            self.save_gate_tables(tables);
            return Ok(WaitOutcome::Blocked);
        }

        let id = gate.queue[gate.queue_head as usize];
        let slot = tables.request_slot_of(id).ok_or(Error::Stale)?;
        let request = tables.slot_request(slot).ok_or(Error::Stale)?;
        if request.phase != Phase::Queued {
            return Err(Error::Stale);
        }
        let promote = gate.capacity_len > 0;
        let events = 2 + usize::from(promote);
        if !self.can_mint_gate_capability() {
            return Err(Error::Capacity);
        }
        self.preflight_gate_audit(events, service_cookie, gate_audit::Operation::Deliver)
            .map_err(|_| Error::AuditFull)?;

        let request_cap = self
            .mint_root_capability(service, REQUEST_ROOT_RIGHTS, ObjectRef::GateRequest(id))
            .expect("wait preflighted capability capacity");
        self.audit.append(AuditEvent::CapabilityMinted {
            cap: request_cap,
            holder: service,
        });
        let mut gate = gate;
        gate.queue_head = (gate.queue_head + 1) % QUEUE_DEPTH as u32;
        gate.queue_len -= 1;
        gate.service_ticket = None;
        if promote {
            let promoted = gate.capacity[gate.capacity_head as usize];
            gate.capacity_head = (gate.capacity_head + 1) % MAX_BINDINGS as u32;
            gate.capacity_len -= 1;
            gate.queue[(gate.queue_head + gate.queue_len) as usize % QUEUE_DEPTH] = promoted;
            gate.queue_len += 1;
            if let Some(promoted_slot) = tables.request_slot_of(promoted)
                && let Some(mut promoted_request) = tables.slot_request(promoted_slot)
            {
                promoted_request.phase = Phase::Queued;
                tables.set_request(promoted_slot, promoted_request);
            }
            self.audit.append(AuditEvent::GatePromoted {
                gate: gate.id,
                request: promoted,
            });
        }
        let mut delivered = request;
        delivered.phase = Phase::Delivered;
        delivered.root_cap = Some(request_cap);
        tables.set_request(slot, delivered);
        tables.write(gate);
        self.save_gate_tables(tables);
        self.audit.append(AuditEvent::GateDelivered {
            gate: gate.id,
            request: id,
            service,
        });
        Ok(WaitOutcome::Delivered {
            request: id,
            request_cap,
            client: request.client,
            client_cookie: request.client_cookie,
            number: request.number,
            args: request.args,
            flags: request.flags,
        })
    }

    // ------------------------------------------------------------------
    // reply / abort
    // ------------------------------------------------------------------

    /// 答复一次已交付请求：完成客户端私有等待、只写客户端保存帧的 `rax`、撤销全部请求
    /// 能力别名并记录终结。`abort` 与 `reply` 搬运同一个不透明 `u64`，只有审计原因不同。
    ///
    /// 整个提交在同一个不可调度区间内完成；重复答复/迟到答复得到 [`Error::Stale`]。
    pub fn gate_reply(
        &mut self,
        service: u32,
        request_cap: Handle,
        value: u64,
        abort: bool,
    ) -> Result<ReplyOutcome, Error> {
        if self.slots.check(request_cap) != SlotState::Live {
            return Err(Error::Stale);
        }
        let service_cookie = self.task_cookie(service).ok_or(Error::Denied)?;
        self.authorize(service, request_cap, Right::GateServe)
            .map_err(|_| Error::Denied)?;
        let Some(ObjectRef::GateRequest(id)) = self.target_of(request_cap) else {
            return Err(Error::Denied);
        };
        let tables = self.gate_tables();
        let Some(slot) = tables.request_slot_of(id) else {
            return Err(Error::Stale);
        };
        let Some(request) = tables.slot_request(slot) else {
            return Err(Error::Stale);
        };
        if request.phase != Phase::Delivered {
            return Err(Error::Stale);
        }
        if self.task_cookie(request.client) != Some(request.client_cookie) {
            return Err(Error::Stale);
        }
        let Some(gate) = tables.find(request.gate).filter(|gate| gate.live) else {
            return Err(Error::Stale);
        };
        if gate.service != service || gate.service_cookie != service_cookie {
            return Err(Error::Denied);
        }
        let ticket = request.ticket.ok_or(Error::Stale)?;
        let root = request.root_cap.ok_or(Error::Stale)?;
        let (revoked, revoked_count) = self.records.revoke_subtree(root);
        // Completed + Ready 迁移 + GateTerminated + 每个请求能力别名的撤销事件。
        let events = 3 + revoked_count;
        self.preflight_gate_audit(events, service_cookie, gate_audit::Operation::Terminate)
            .map_err(|_| Error::AuditFull)?;

        self.complete_completion_wait(ticket)
            .expect("delivered client holds the reserved wait");
        // 只改写客户端保存帧的 rax；其余寄存器保持。
        crate::sched::set_frame_rax(request.client, value);
        self.audit.append(AuditEvent::GateTerminated {
            gate: gate.id,
            request: id,
            value,
            reason: if abort {
                Reason::Aborted
            } else {
                Reason::Replied
            },
        });
        for cap in revoked[..revoked_count].iter().copied() {
            self.retire_capability(cap);
            self.audit.append(AuditEvent::CapabilityRevoked { cap });
        }
        let mut request = request;
        request.phase = Phase::Idle;
        request.id = 0;
        request.ticket = None;
        request.root_cap = None;
        request.terminal = value;
        tables.set_request(slot, request);
        self.save_gate_tables(tables);
        Ok(ReplyOutcome {
            client: request.client,
            client_cookie: request.client_cookie,
            value,
        })
    }

    /// `gate_abort`：与 [`Self::gate_reply`] 同一提交路径，审计原因不同。
    pub fn gate_abort(
        &mut self,
        service: u32,
        request_cap: Handle,
        value: u64,
    ) -> Result<ReplyOutcome, Error> {
        self.gate_reply(service, request_cap, value, true)
    }

    // ------------------------------------------------------------------
    // 清理
    // ------------------------------------------------------------------

    fn remove_from_pending(gate: &mut GateRecord, id: u32) {
        let mut queue = [0u32; QUEUE_DEPTH];
        let mut len = 0usize;
        for index in 0..gate.queue_len as usize {
            let value = gate.queue[(gate.queue_head as usize + index) % QUEUE_DEPTH];
            if value != id {
                queue[len] = value;
                len += 1;
            }
        }
        gate.queue = queue;
        gate.queue_head = 0;
        gate.queue_len = len as u32;
        let mut capacity = [0u32; MAX_BINDINGS];
        let mut len = 0usize;
        for index in 0..gate.capacity_len as usize {
            let value = gate.capacity[(gate.capacity_head as usize + index) % MAX_BINDINGS];
            if value != id {
                capacity[len] = value;
                len += 1;
            }
        }
        gate.capacity = capacity;
        gate.capacity_head = 0;
        gate.capacity_len = len as u32;
    }

    /// 任务销毁时门控清理的审计条数（在 `destroy_audit_events` 中一次性预检）。
    pub(super) fn gate_cleanup_audit_events(&self, task: u32) -> usize {
        let tables = self.gate_tables();
        let mut events = 0usize;
        if let Some(binding) = self.gate_binding_of(&tables, task) {
            events += 1; // GateDetached
            if let Some(request) = tables.slot_request(binding.slot)
                && request.phase != Phase::Idle
            {
                events += 1; // GateCancelled
                if let Some(root) = request.root_cap {
                    events += self.records.revoke_subtree(root).1;
                }
            }
            if let Some(gate) = tables.find(binding.gate)
                && gate.live
                && gate.capacity_len > 0
            {
                events += 1; // GatePromoted
            }
        }
        for id in 1..=tables.gate_count {
            let Some(gate) = tables.find(id) else {
                continue;
            };
            if gate.service != task {
                continue;
            }
            events += 1; // GateOrphaned
            for slot in 0..MAX_BINDINGS as u32 {
                let Some(request) = tables.slot_request(slot) else {
                    continue;
                };
                if request.gate != gate.id || request.phase == Phase::Idle {
                    continue;
                }
                // Completed + Ready 迁移 + GateTerminated，以及可能存在的请求能力别名。
                events += 3;
                if let Some(root) = request.root_cap {
                    events += self.records.revoke_subtree(root).1;
                }
            }
        }
        events
    }

    /// 任务销毁前的门控清理：客户端销毁取消请求且**不恢复帧**；服务者销毁把门控孤儿化，
    /// 所有未决请求以 `attach` 配置值终结。调用方必须已把
    /// [`Self::gate_cleanup_audit_events`] 计入审计预检。
    pub(super) fn gate_on_task_destroy(&mut self, task: u32) {
        let mut tables = self.gate_tables();

        // 1) 作为客户端：解绑、取消请求（不恢复帧）、FIFO 晋级。
        if let Some(binding) = self.gate_binding_of(&tables, task) {
            let mut cancelled = 0u32;
            if let Some(request) = tables.slot_request(binding.slot) {
                if request.phase != Phase::Idle {
                    cancelled = request.id;
                    if let Some(root) = request.root_cap {
                        let (revoked, count) = self.records.revoke_subtree(root);
                        for cap in revoked[..count].iter().copied() {
                            self.retire_capability(cap);
                            self.audit.append(AuditEvent::CapabilityRevoked { cap });
                        }
                    }
                    self.audit.append(AuditEvent::GateCancelled {
                        gate: binding.gate,
                        request: request.id,
                    });
                    // 客户端拥有的等待对象由 destroy_completion_waits 统一回收。
                } else {
                    let _ = self.release_completion_wait(binding.wait);
                }
                tables.clear_request(binding.slot);
            }
            tables.set_binding(binding.slot, None);
            tables.binding_count = tables.binding_count.saturating_sub(1);
            if let Some(mut gate) = tables.find(binding.gate) {
                gate.bindings = gate.bindings.saturating_sub(1);
                if cancelled != 0 {
                    Self::remove_from_pending(&mut gate, cancelled);
                }
                if gate.live && gate.capacity_len > 0 {
                    let promoted = gate.capacity[gate.capacity_head as usize];
                    gate.capacity_head = (gate.capacity_head + 1) % MAX_BINDINGS as u32;
                    gate.capacity_len -= 1;
                    gate.queue[(gate.queue_head + gate.queue_len) as usize % QUEUE_DEPTH] =
                        promoted;
                    gate.queue_len += 1;
                    if let Some(slot) = tables.request_slot_of(promoted)
                        && let Some(mut request) = tables.slot_request(slot)
                    {
                        request.phase = Phase::Queued;
                        tables.set_request(slot, request);
                    }
                    self.audit.append(AuditEvent::GatePromoted {
                        gate: gate.id,
                        request: promoted,
                    });
                }
                tables.write(gate);
            }
            self.audit.append(AuditEvent::GateDetached {
                gate: binding.gate,
                client: task,
            });
        }

        // 2) 作为固定服务者：孤儿化，并以配置值终结全部未决请求。
        for id in 1..=tables.gate_count {
            let Some(gate) = tables.find(id) else {
                continue;
            };
            if gate.service != task || !gate.live {
                continue;
            }
            let mut terminated = 0u32;
            for slot in 0..MAX_BINDINGS as u32 {
                let Some(request) = tables.slot_request(slot) else {
                    continue;
                };
                if request.gate != gate.id || request.phase == Phase::Idle {
                    continue;
                }
                let orphan = tables.binding(slot).map_or(0, |binding| binding.orphan);
                if let Some(ticket) = request.ticket {
                    // 请求者恢复时从保存帧返回配置值（只改 rax）。
                    crate::sched::set_frame_rax(request.client, orphan);
                    self.complete_completion_wait(ticket)
                        .expect("cleanup preflighted the client wake");
                }
                if let Some(root) = request.root_cap {
                    let (revoked, count) = self.records.revoke_subtree(root);
                    for cap in revoked[..count].iter().copied() {
                        self.retire_capability(cap);
                        self.audit.append(AuditEvent::CapabilityRevoked { cap });
                    }
                }
                self.audit.append(AuditEvent::GateTerminated {
                    gate: gate.id,
                    request: request.id,
                    value: orphan,
                    reason: Reason::Orphaned,
                });
                tables.clear_request(slot);
                terminated += 1;
            }
            let mut gate = gate;
            gate.live = false;
            gate.service_ticket = None;
            gate.service_wait = None;
            gate.queue = [0; QUEUE_DEPTH];
            gate.queue_head = 0;
            gate.queue_len = 0;
            gate.capacity = [0; MAX_BINDINGS];
            gate.capacity_head = 0;
            gate.capacity_len = 0;
            tables.write(gate);
            self.audit.append(AuditEvent::GateOrphaned {
                gate: gate.id,
                requests: terminated,
            });
        }
        self.save_gate_tables(tables);
    }

    /// W17 风格投影：绑定/请求/队列/等待登记的双向一致性（host 单测使用）。
    pub fn gate_well_formed(&self) -> bool {
        let tables = self.gate_tables();
        let mut binding_count = 0u32;
        for slot in 0..MAX_BINDINGS as u32 {
            let binding = tables.binding(slot);
            let request = tables.slot_request(slot);
            match (binding, request) {
                (Some(binding), Some(request)) => {
                    if !binding.active || binding.slot != slot {
                        return false;
                    }
                    if self.task_cookie(binding.task) != Some(binding.cookie) {
                        return false;
                    }
                    if request.binding != slot {
                        return false;
                    }
                    if request.phase == Phase::Idle {
                        if request.ticket.is_some() || request.root_cap.is_some() {
                            return false;
                        }
                    } else {
                        if request.gate != binding.gate
                            || request.client != binding.task
                            || request.client_cookie != binding.cookie
                        {
                            return false;
                        }
                        if request.ticket.is_none() {
                            return false;
                        }
                        if request.phase == Phase::Delivered && request.root_cap.is_none() {
                            return false;
                        }
                        if matches!(request.phase, Phase::Queued | Phase::WaitingCapacity) {
                            let Some(gate) = tables.find(request.gate) else {
                                return false;
                            };
                            let queued = (0..gate.queue_len as usize).any(|index| {
                                gate.queue[(gate.queue_head as usize + index) % QUEUE_DEPTH]
                                    == request.id
                            });
                            let waiting = (0..gate.capacity_len as usize).any(|index| {
                                gate.capacity[(gate.capacity_head as usize + index) % MAX_BINDINGS]
                                    == request.id
                            });
                            if queued == waiting {
                                return false;
                            }
                        }
                    }
                    binding_count += 1;
                }
                (None, Some(request)) if request.phase == Phase::Idle => {}
                (None, None) => {}
                _ => return false,
            }
        }
        if binding_count != tables.binding_count {
            return false;
        }
        for id in 1..=tables.gate_count {
            let Some(gate) = tables.find(id) else {
                return false;
            };
            if gate.bindings != tables.bound_count(id) {
                return false;
            }
            if gate.live {
                if self.task_cookie(gate.service) != Some(gate.service_cookie) {
                    return false;
                }
                if !self.cap_target_live(gate.root_cap) {
                    return false;
                }
                if gate.queue_len as usize > QUEUE_DEPTH
                    || gate.queue_head as usize >= QUEUE_DEPTH
                    || gate.capacity_len as usize > MAX_BINDINGS
                {
                    return false;
                }
            } else if gate.queue_len != 0 || gate.capacity_len != 0 {
                return false;
            }
        }
        self.completion_waits_well_formed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 门控测试写入调度器的保存帧（reply 只改 `rax`），故串行化，避免并行用例互相覆盖。
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn state() -> BootstrapState {
        super::completion::tests::state()
    }

    /// 服务者 2（Running）对客户端 3（Ready）持有 `TaskManageVSpace`。
    fn pair() -> BootstrapState {
        let mut state = state();
        state.create_task(3).unwrap();
        state
            .mint_root_capability(
                2,
                Rights::from_rights(&[Right::Derive, Right::TaskManageVSpace]),
                ObjectRef::Task(3),
            )
            .unwrap();
        state
    }

    fn task_cap(state: &BootstrapState, client: u32) -> Handle {
        for index in 1..MAX_CAPABILITY_SLOTS as u32 {
            let handle = Handle::new(index, 1).unwrap();
            if state.target_of(handle) == Some(ObjectRef::Task(client))
                && state.holder_of(handle) == Some(2)
            {
                return handle;
            }
        }
        panic!("client task cap minted");
    }

    fn attach(state: &mut BootstrapState, client: u32, orphan: u64) -> AttachOutcome {
        state
            .gate_attach(2, task_cap(state, client), Handle::ZERO, orphan)
            .unwrap()
    }

    #[test]
    fn attach_reserves_waits_and_a_request_slot_atomically() {
        let _serial = serial();
        let mut state = pair();
        let audit = state.audit.count;
        let live = state.slots.live_count();
        let out = attach(&mut state, 3, 0xdead);
        assert_eq!(
            state.audit.count,
            audit + 5,
            "GateCreated/Cap/2 reservations/Attached"
        );
        assert_eq!(state.slots.live_count(), live + 1);
        assert_eq!(state.gate_tables().binding_count, 1);
        assert_eq!(state.completion_wait_count(2), 1, "service wait");
        assert_eq!(state.completion_wait_count(3), 1, "client wait");
        assert!(state.gate_well_formed());
        let gate = state.gate_tables().find(out.gate).unwrap();
        assert!(gate.live);
        assert_eq!(gate.service, 2);
        assert_eq!(gate.bindings, 1);
        let rights = state.rights_of(out.gate_cap).unwrap();
        assert!(rights.contains(Right::GateAttach));
        assert!(rights.contains(Right::GateServe));
        assert_eq!(state.tasks.status_of(3), Some(TaskStatus::Ready));
        let request = state.gate_tables().slot_request(out.slot).unwrap();
        assert_eq!(request.phase, Phase::Idle);
        assert!(request.ticket.is_none());
    }

    #[test]
    fn attach_rejects_a_non_ready_target_without_partial_state() {
        let _serial = serial();
        let mut state = pair();
        state.tasks.find_mut(3).unwrap().status = TaskStatus::BlockedWait;
        let before = (
            state.audit.count,
            state.slots.live_count(),
            state.gate_tables(),
            state.wait_objects.count,
        );
        assert_eq!(
            state.gate_attach(2, task_cap(&state, 3), Handle::ZERO, 0),
            Err(Error::Busy)
        );
        assert_eq!(
            before,
            (
                state.audit.count,
                state.slots.live_count(),
                state.gate_tables(),
                state.wait_objects.count,
            )
        );
        assert_eq!(state.gate_tables().binding_count, 0);
        assert_eq!(state.gate_tables().gate_count, 0);
    }

    #[test]
    fn trap_delivers_the_opaque_words_and_reply_writes_only_rax() {
        let _serial = serial();
        let mut state = pair();
        let out = attach(&mut state, 3, 0xdead);
        state.yield_task(2).unwrap();
        state.schedule_task(3).unwrap();
        let args = [11, 22, 33, 44, 55, 66];
        assert_eq!(
            state.gate_trap(3, 0x1234, args, 0).unwrap(),
            TrapOutcome::Queued { request: 1 }
        );
        assert_eq!(state.tasks.status_of(3), Some(TaskStatus::BlockedWait));
        assert!(state.gate_well_formed());
        state.schedule_task(2).unwrap();
        let delivered = state.gate_wait(2, out.gate_cap).unwrap();
        let WaitOutcome::Delivered {
            request_cap,
            number,
            args: seen,
            flags,
            ..
        } = delivered
        else {
            panic!("queue was non-empty");
        };
        assert_eq!(number, 0x1234);
        assert_eq!(seen, args);
        assert_eq!(flags, 0);
        let words = delivered.words().unwrap();
        assert_eq!(words[0], VERSION);
        assert_eq!(words[1], OUTPUT_BYTES);
        assert_eq!(words[2], request_cap.encode());
        assert_eq!(&words[4..11], &[0x1234, 11, 22, 33, 44, 55, 66][..]);
        crate::sched::set_frame(3, crate::exc::SretFrame::ZERO);
        let reply = state.gate_reply(2, request_cap, 0xfeed, false).unwrap();
        assert_eq!(reply.client, 3);
        assert_eq!(reply.value, 0xfeed);
        assert_eq!(state.tasks.status_of(3), Some(TaskStatus::Ready));
        assert_eq!(crate::sched::frame(3).unwrap().rax, 0xfeed);
        assert_eq!(
            state.gate_reply(2, request_cap, 0xfeed, false),
            Err(Error::Stale),
            "答复至多一次"
        );
        assert!(state.gate_well_formed());
        // 同一绑定下一次 trap 使用新的请求 id，不复用旧 id。
        state.yield_task(2).unwrap();
        state.schedule_task(3).unwrap();
        let next = state.gate_trap(3, 7, [0; 6], 0).unwrap();
        assert_eq!(next, TrapOutcome::Queued { request: 2 });
    }

    #[test]
    fn empty_queue_blocks_the_service_and_an_enqueue_wakes_it_once() {
        let _serial = serial();
        let mut state = pair();
        let out = attach(&mut state, 3, 0xdead);
        assert_eq!(
            state.gate_wait(2, out.gate_cap).unwrap(),
            WaitOutcome::Blocked
        );
        assert_eq!(state.tasks.status_of(2), Some(TaskStatus::BlockedWait));
        state.schedule_task(3).unwrap();
        assert_eq!(
            state.gate_trap(3, 5, [0; 6], 0).unwrap(),
            TrapOutcome::Queued { request: 1 }
        );
        assert_eq!(
            state.tasks.status_of(2),
            Some(TaskStatus::Ready),
            "入队只唤醒服务等待"
        );
        state.schedule_task(2).unwrap();
        assert!(matches!(
            state.gate_wait(2, out.gate_cap).unwrap(),
            WaitOutcome::Delivered { request: 1, .. }
        ));
        assert!(state.gate_well_formed());
    }

    #[test]
    fn service_death_orphans_requests_with_the_configured_value() {
        let _serial = serial();
        let mut state = pair();
        state.create_task(4).unwrap();
        state
            .mint_root_capability(
                2,
                Rights::from_rights(&[Right::Derive, Right::TaskManageVSpace]),
                ObjectRef::Task(4),
            )
            .unwrap();
        let out = attach(&mut state, 3, 0x111);
        state
            .gate_attach(2, task_cap(&state, 4), out.gate_cap, 0x222)
            .unwrap();
        // 3 入队；2 取走并交付；4 入队（等待服务）。
        state.yield_task(2).unwrap();
        state.schedule_task(3).unwrap();
        state.gate_trap(3, 1, [0; 6], 0).unwrap();
        state.schedule_task(2).unwrap();
        let delivered = state.gate_wait(2, out.gate_cap).unwrap();
        let WaitOutcome::Delivered { request_cap, .. } = delivered else {
            panic!()
        };
        state.yield_task(2).unwrap();
        state.schedule_task(4).unwrap();
        state.gate_trap(4, 2, [0; 6], 0).unwrap();
        // 服务者当前 Ready：销毁即孤儿化。
        state.destroy_task(2).unwrap();
        assert!(!state.gate_tables().find(out.gate).unwrap().live);
        assert_eq!(state.tasks.status_of(3), Some(TaskStatus::Ready));
        assert_eq!(state.tasks.status_of(4), Some(TaskStatus::Ready));
        assert_eq!(crate::sched::frame(3).unwrap().rax, 0x111);
        assert_eq!(crate::sched::frame(4).unwrap().rax, 0x222);
        assert_eq!(
            state.gate_reply(2, request_cap, 0, false),
            Err(Error::Stale),
            "孤儿化后迟到答复确定性失败"
        );
        // 绑定保留为 fail-closed：再次 trap 得到配置值且不查原生分派。
        state.schedule_task(3).unwrap();
        assert_eq!(
            state.gate_trap(3, 9, [0; 6], 0).unwrap(),
            TrapOutcome::Orphaned { value: 0x111 }
        );
        assert!(state.gate_well_formed());
    }

    #[test]
    fn client_destruction_cancels_without_restoring_the_frame() {
        let _serial = serial();
        let mut state = pair();
        let out = attach(&mut state, 3, 0xdead);
        state.yield_task(2).unwrap();
        state.schedule_task(3).unwrap();
        state.gate_trap(3, 1, [0; 6], 0).unwrap();
        state.schedule_task(2).unwrap();
        let delivered = state.gate_wait(2, out.gate_cap).unwrap();
        let WaitOutcome::Delivered { request_cap, .. } = delivered else {
            panic!()
        };
        crate::sched::set_frame(3, crate::exc::SretFrame::ZERO);
        crate::sched::set_frame_rax(3, 0xaaaa);
        state.destroy_task(3).unwrap();
        assert_eq!(
            crate::sched::frame(3).unwrap().rax,
            0xaaaa,
            "请求者销毁不恢复帧、不写答复值"
        );
        assert_eq!(
            state.gate_reply(2, request_cap, 7, false),
            Err(Error::Stale),
            "请求能力随请求者销毁失效"
        );
        assert_eq!(state.gate_tables().binding_count, 0);
        assert_eq!(state.gate_tables().find(out.gate).unwrap().bindings, 0);
        assert!(state.gate_well_formed());
    }

    #[test]
    fn full_queue_blocks_capacity_then_promotes_in_fifo_order() {
        let _serial = serial();
        let mut state = pair();
        let first = attach(&mut state, 3, 0);
        for client in 4..=11 {
            state.create_task_record(client, 0).unwrap();
            state
                .mint_root_capability(
                    2,
                    Rights::from_rights(&[Right::Derive, Right::TaskManageVSpace]),
                    ObjectRef::Task(client),
                )
                .unwrap();
            state
                .gate_attach(2, task_cap(&state, client), first.gate_cap, 0)
                .unwrap();
        }
        state.yield_task(2).unwrap();
        for client in 3..=11 {
            state.schedule_task(client).unwrap();
            state
                .gate_trap(client, 0x100 + client as u64, [0; 6], 0)
                .unwrap();
        }
        let tables = state.gate_tables();
        let gate = tables.find(first.gate).unwrap();
        assert_eq!(gate.queue_len, QUEUE_DEPTH as u32);
        assert_eq!(gate.capacity_len, 1, "第 9 个 trap 进入容量等待");
        assert_eq!(
            tables.request(9).map(|request| request.phase),
            Some(Phase::WaitingCapacity)
        );
        state.schedule_task(2).unwrap();
        assert!(matches!(
            state.gate_wait(2, first.gate_cap).unwrap(),
            WaitOutcome::Delivered { request: 1, .. }
        ));
        let tables = state.gate_tables();
        let gate = tables.find(first.gate).unwrap();
        assert_eq!(gate.queue_len, QUEUE_DEPTH as u32);
        assert_eq!(gate.capacity_len, 0);
        assert_eq!(
            tables.request(9).map(|request| request.phase),
            Some(Phase::Queued),
            "FIFO 晋级容量等待者"
        );
        assert!(state.gate_well_formed());
    }

    #[test]
    fn reply_rejects_foreign_holders_and_accepted_aliases_fail_together() {
        let _serial = serial();
        let mut state = pair();
        let out = attach(&mut state, 3, 0);
        state.yield_task(2).unwrap();
        state.schedule_task(3).unwrap();
        state.gate_trap(3, 1, [0; 6], 0).unwrap();
        state.schedule_task(2).unwrap();
        let WaitOutcome::Delivered { request_cap, .. } = state.gate_wait(2, out.gate_cap).unwrap()
        else {
            panic!()
        };
        let alias = state
            .derive_capability(2, request_cap, REQUEST_ROOT_RIGHTS)
            .unwrap();
        state.create_task(4).unwrap();
        state.move_capability(alias, 2, 4).unwrap();
        assert_eq!(
            state.gate_reply(4, alias, 1, false),
            Err(Error::Denied),
            "非固定服务者不得答复"
        );
        assert!(state.gate_reply(2, request_cap, 9, false).is_ok());
        for cap in [request_cap, alias] {
            assert_eq!(
                state.gate_reply(2, cap, 9, false),
                Err(Error::Stale),
                "请求终结后所有别名失效"
            );
        }
        assert!(state.gate_well_formed());
    }

    #[test]
    fn audit_exhaustion_fails_stop_before_any_gate_mutation() {
        let _serial = serial();
        let mut state = pair();
        let reservation = MAX_AUDIT_EVENTS - state.audit.count - 2;
        assert!(state.audit.reserve(reservation));
        let before = (state.audit.count, state.slots, state.gate_tables());
        assert_eq!(
            state.gate_attach(2, task_cap(&state, 3), Handle::ZERO, 0),
            Err(Error::AuditFull)
        );
        assert_eq!(
            before,
            (state.audit.count, state.slots, state.gate_tables())
        );
        assert_eq!(state.gate_tables().binding_count, 0);
        assert!(state.gate_audit_diagnostic().is_some());
        assert_eq!(state.wait_objects.count, 0);
    }

    #[test]
    fn well_formed_projection_detects_a_broken_binding() {
        let _serial = serial();
        let mut state = pair();
        let out = attach(&mut state, 3, 0);
        assert!(state.gate_well_formed());
        let tables = state.gate_tables();
        let mut binding = tables.binding(out.slot).unwrap();
        binding.cookie ^= 1;
        tables.set_binding(out.slot, Some(binding));
        assert!(!state.gate_well_formed());
    }
}
