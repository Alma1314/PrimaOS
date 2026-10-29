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
    /// SPEC-102：运行期指派给新 Task 的**服务名/身份令牌**，按值写入其初始帧 `rcx`（第 4 个入口
    /// 参数）。与内核 `TaskCreateRequest::name`（SPEC-101 的第 11 个 qword）对应。
    pub name: u64,
}

/// SPEC-102：`task_create` 请求块的字数。必须与内核 `syscall::TASK_CREATE_REQ_QWORDS` 一致
/// （SPEC-101 起为 11：第 11 个 qword 是 [`TaskSpec::name`]）。
pub const TASK_CREATE_WORDS: usize = 11;

/// 按内核 `TaskCreateRequest` 的固定顺序把 [`TaskSpec`] 编成请求块字数组。
fn task_create_words(spec: &TaskSpec) -> [u64; TASK_CREATE_WORDS] {
    [
        spec.code_segment.as_raw(),
        spec.code_offset,
        spec.code_pages,
        spec.load_base,
        spec.entry,
        spec.stack_top,
        spec.parent_namespace.map_or(0, Capability::as_raw),
        spec.root_rights.bits(),
        spec.initial_capabilities[0].map_or(0, Capability::as_raw),
        spec.initial_capabilities[1].map_or(0, Capability::as_raw),
        spec.name,
    ]
}

impl<T: SyscallTransport> Runtime<T> {
    /// 查询当前运行 Task 的 ID。
    pub fn task_id(&self) -> Result<TaskId> {
        let raw = self.call(syscall::TASK_ID, [0; 6])?;
        let id = u32::try_from(raw).map_err(|_| crate::Error::InvalidResponse)?;
        Ok(TaskId::from_raw(id))
    }

    /// Process leader identity; threads in one Process return the same ID.
    pub fn process_id(&self) -> Result<TaskId> {
        let raw = self.call(syscall::PROCESS_ID, [0; 6])?;
        Ok(TaskId::from_raw(
            u32::try_from(raw).map_err(|_| crate::Error::InvalidResponse)?,
        ))
    }

    pub fn parent_process_id(&self) -> Result<Option<TaskId>> {
        let raw = self.call(syscall::PROCESS_ID, [1, 0, 0, 0, 0, 0])?;
        let id = u32::try_from(raw).map_err(|_| crate::Error::InvalidResponse)?;
        Ok((id != 0).then_some(TaskId::from_raw(id)))
    }

    /// Read a live Task incarnation cookie (generation in the high 32 bits).
    pub fn task_cookie(&self, task: TaskId) -> Result<u64> {
        self.call(syscall::TASK_INFO, [1, task.as_raw() as u64, 0, 0, 0, 0])
    }
    pub fn task_process_cookie(&self, task: TaskId) -> Result<u64> {
        self.call(syscall::TASK_INFO, [2, task.as_raw() as u64, 0, 0, 0, 0])
    }
    pub fn task_parent_cookie(&self, task: TaskId) -> Result<u64> {
        self.call(syscall::TASK_INFO, [3, task.as_raw() as u64, 0, 0, 0, 0])
    }
    pub fn task_prepared(&self, task: TaskId) -> Result<bool> {
        match self.call(syscall::TASK_INFO, [6, task.as_raw() as u64, 0, 0, 0, 0])? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(crate::Error::InvalidResponse),
        }
    }
    pub fn send_target_cookie(&self, cap: Capability) -> Result<u64> {
        self.call(syscall::TASK_INFO, [4, cap.as_raw(), 0, 0, 0, 0])
    }
    pub fn self_capability(&self) -> Result<Capability> {
        Capability::from_raw(self.call(syscall::TASK_INFO, [5, 0, 0, 0, 0, 0])?)
    }

    /// 创建 Task，返回管理该 Task 的 Capability。
    ///
    /// 成功时 `initial_capabilities` 中的 token 转移给新 Task；调用失败时不发生部分转移。
    pub fn task_create(&self, spec: &TaskSpec) -> Result<Capability> {
        let words = task_create_words(spec);
        let raw = self.call(
            syscall::TASK_CREATE,
            [words.as_ptr() as usize as u64, 0, 0, 0, 0, 0],
        )?;
        Capability::from_raw(raw)
    }

    /// Terminate all threads in the Process named by its leader capability.
    pub fn process_terminate(&self, leader: Capability, status: u8) -> Result<()> {
        self.call_unit(
            syscall::PROCESS_TERMINATE,
            [leader.as_raw(), status as u64, 0, 0, 0, 0],
        )
    }

    /// 销毁持有 Capability 所指向的 Task。
    pub fn task_destroy(&self, task: Capability) -> Result<()> {
        self.call_unit(syscall::TASK_DESTROY, [task.as_raw(), 0, 0, 0, 0, 0])
    }
}

#[cfg(test)]
mod tests {
    use super::{TASK_CREATE_WORDS, TaskSpec, task_create_words};
    use crate::{Capability, Rights};

    #[test]
    fn task_create_request_carries_name_as_the_last_word() {
        let spec = TaskSpec {
            code_segment: Capability::from_raw(0x0000_0001_0000_002d).unwrap(),
            code_offset: 3,
            code_pages: 2,
            load_base: 0x1000,
            entry: 0x1234,
            stack_top: 0x9000,
            parent_namespace: None,
            root_rights: Rights::from_bits(0),
            initial_capabilities: [None, None],
            name: 0x0000_0000_6174_6562, // "beta"
        };
        let words = task_create_words(&spec);
        // 块长必须与内核 `TASK_CREATE_REQ_QWORDS` 一致（SPEC-102 修越界缺口）。
        assert_eq!(words.len(), TASK_CREATE_WORDS);
        assert_eq!(words.len(), 11);
        // 第 11 个 word 必须是 `name`（SPEC-101 起内核在此读名字）。
        assert_eq!(words[10], 0x0000_0000_6174_6562);
        // 相邻字段不得错位。
        assert_eq!(words[1], 3);
        assert_eq!(words[2], 2);
        assert_eq!(words[5], 0x9000);
        assert_eq!(words[9], 0);
    }
}
