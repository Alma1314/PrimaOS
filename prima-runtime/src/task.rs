//! Task 生命周期操作。

use crate::{Capability, Result, Rights, SyscallTransport, syscall, transport::Runtime};

/// 内核分配的 Task ID；它用于寻址，不授予 Task 管理权限。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct TaskId(u32);

impl TaskId {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

/// 创建子 Task 所需的映像、入口、栈和初始 Authority。
#[derive(Clone, Copy, Debug)]
pub struct TaskSpec {
    pub code_segment: Capability,
    pub code_offset: u64,
    pub code_pages: u64,
    pub load_base: u64,
    pub entry: u64,
    pub stack_top: u64,
    pub parent_namespace: Option<Capability>,
    pub root_rights: Rights,
    pub initial_capabilities: [Option<Capability>; 2],
}

impl<T: SyscallTransport> Runtime<T> {
    /// 查询当前运行 Task 的 ID。
    pub fn task_id(&self) -> Result<TaskId> {
        let raw = self.call(syscall::TASK_ID, [0; 6])?;
        let id = u32::try_from(raw).map_err(|_| crate::Error::InvalidResponse)?;
        Ok(TaskId::from_raw(id))
    }

    /// 创建 Task，返回管理该 Task 的 Capability。
    ///
    /// 成功时 `initial_capabilities` 中的 token 转移给新 Task；调用失败时不发生部分转移。
    pub fn task_create(&self, spec: &TaskSpec) -> Result<Capability> {
        let words = [
            spec.code_segment.as_raw(),
            spec.code_offset,
            spec.code_pages,
            spec.load_base,
            spec.entry,
            spec.stack_top,
            spec.parent_namespace.map_or(0, Capability::as_raw),
            spec.root_rights.bits() as u64,
            spec.initial_capabilities[0].map_or(0, Capability::as_raw),
            spec.initial_capabilities[1].map_or(0, Capability::as_raw),
        ];
        let raw = self.call(
            syscall::TASK_CREATE,
            [words.as_ptr() as usize as u64, 0, 0, 0, 0, 0],
        )?;
        Capability::from_raw(raw)
    }

    /// 销毁持有 Capability 所指向的 Task。
    pub fn task_destroy(&self, task: Capability) -> Result<()> {
        self.call_unit(syscall::TASK_DESTROY, [task.as_raw(), 0, 0, 0, 0, 0])
    }
}
