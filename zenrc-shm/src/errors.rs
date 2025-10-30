use thiserror::Error;

#[derive(Debug, Error)]
pub enum MutexLockError {
    #[error("Mutex failed init with code {0}")]
    InitError(i32),
    #[error("Failed to lock Mutex with code {0}")]
    LockError(i32),
    #[error("Try lock Mutex failed with code {0}")]
    TryLockError(i32),
    #[error("Failed to unlock Mutex with code {0}")]
    UnlockError(i32),
    #[error("Timeout while trying to lock Mutex with code {0}")]
    TimeoutError(i32),
}

#[derive(Debug, Error)]
pub enum RwLockError {
    #[error("RwLock failed init with code {0}")]
    InitError(i32),
    #[error("Failed to read RwLock with code {0}")]
    ReadLockError(i32),
    #[error("Try read RwLock failed with code {0}")]
    TryReadLockError(i32),
    #[error("Failed to write RwLock with code {0}")]
    WriteLockError(i32),
    #[error("Try write RwLock failed with code {0}")]
    TryWriteLockError(i32),
    #[error("Failed to unlock Read RwLock with code {0}")]
    ReadUnlockError(i32),
    #[error("Failed to unlock Write RwLock with code {0}")]
    WriteUnlockError(i32),
    #[error("Try into SharedRwLock failed due to invalid pointer")]
    IntoError,
	#[error("RwLock is empty, no data to read")]
	Empty,
}
