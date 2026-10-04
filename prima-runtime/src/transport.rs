//! 用户态 syscall 传输抽象与首批 Runtime 调用。

use crate::{Error, Result, TransportError, capability::Capability, syscall};

/// 将 Runtime 调用递送到当前任务内核入口的适配器。
///
/// 实现负责遵循目标平台调用约定，并返回原始 `rax` 位模式。它不能授予 Capability；
/// 权限检查由内核完成。调用必须同步完成，不能在 `invoke` 返回后继续使用 syscall 参数中
/// 的用户缓冲区指针。该 trait 保持公开，便于目标平台适配和 host 侧协议模拟。
pub trait SyscallTransport {
    /// 以调用号和最多六个机器字参数发起一次内核调用。
    fn invoke(&self, number: u64, args: [u64; 6]) -> core::result::Result<u64, TransportError>;
}

/// 通过显式传入的传输适配器调用 Prima 内核服务。
pub struct Runtime<T> {
    transport: T,
}

impl<T: SyscallTransport> Runtime<T> {
    /// 创建 Runtime。此操作不取得或创建任何 Capability。
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    /// 拿回底层传输适配器。
    pub fn into_transport(self) -> T {
        self.transport
    }

    pub(crate) fn call(&self, number: u64, args: [u64; 6]) -> Result<u64> {
        let raw = self
            .transport
            .invoke(number, args)
            .map_err(Error::Transport)?;
        syscall::decode(raw)
    }

    pub(crate) fn call_unit(&self, number: u64, args: [u64; 6]) -> Result<()> {
        match self.call(number, args)? {
            0 => Ok(()),
            _ => Err(Error::InvalidResponse),
        }
    }

    /// 返回当前内核单调时间值。单位由平台实现定义；当前 QEMU 原型使用 PIT tick。
    pub fn time_now(&self) -> Result<u64> {
        self.call(syscall::TIME_NOW, [0; 6])
    }

    /// 创建一个 Timer，并返回调用者持有的 Timer Capability。
    pub fn timer_set(&self, deadline: u64) -> Result<Capability> {
        let raw = self.call(syscall::TIMER_SET, [deadline, 0, 0, 0, 0, 0])?;
        Capability::from_raw(raw)
    }

    /// 取消 Timer。`0` 表示已取消，`1` 表示已触发，`2` 表示此前已取消。
    pub fn timer_cancel(&self, timer: Capability) -> Result<TimerCancelOutcome> {
        match self.call(syscall::TIMER_CANCEL, [timer.as_raw(), 0, 0, 0, 0, 0])? {
            0 => Ok(TimerCancelOutcome::Cancelled),
            1 => Ok(TimerCancelOutcome::AlreadyFired),
            2 => Ok(TimerCancelOutcome::AlreadyCancelled),
            _ => Err(Error::InvalidResponse),
        }
    }

    /// 销毁 Timer 对象。成功后，该 Timer Capability 不再授权 Timer 操作。
    pub fn timer_destroy(&self, timer: Capability) -> Result<()> {
        self.call_unit(syscall::TIMER_DESTROY, [timer.as_raw(), 0, 0, 0, 0, 0])
    }

    /// 创建 Namespace，并返回新根 Capability。
    pub fn namespace_create(&self) -> Result<Capability> {
        let raw = self.call(syscall::NAMESPACE_CREATE, [0; 6])?;
        Capability::from_raw(raw)
    }

    /// 显式丢弃当前持有位置中的 Capability。
    ///
    /// Runtime 不实现 `Drop` 自动释放，因为 token 可能已移动到消息或 Namespace 中；
    /// 调用方必须按当前 Authority 位置显式决定何时释放。
    pub fn cap_drop(&self, capability: Capability) -> Result<()> {
        self.call_unit(syscall::CAP_DROP, [capability.as_raw(), 0, 0, 0, 0, 0])
    }

    /// 让出当前 Task。函数只在该 Task 再次运行并从调用点恢复后返回。
    pub fn yield_task(&self) -> Result<()> {
        self.call_unit(syscall::YIELD, [0; 6])
    }
}

/// `timer_cancel` 的状态结果；`AlreadyFired` 和 `AlreadyCancelled` 不是错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerCancelOutcome {
    Cancelled,
    AlreadyFired,
    AlreadyCancelled,
}
