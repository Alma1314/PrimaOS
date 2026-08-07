#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeError {
    NoPendingException,
    UnrecoverableFault,
    InvalidFrameAddr,
    NotExecutable,
    MissingWrite,
    InvalidExcSource,
    NotSubscribed,
    AlreadyResumedStub, // 占位，Task 4 移除
}
