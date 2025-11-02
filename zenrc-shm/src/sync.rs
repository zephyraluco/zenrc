/**
 * 共享内存段的元数据信息
 */
use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use nix::libc::{
    PTHREAD_PROCESS_SHARED, pthread_cond_t, pthread_mutex_t, pthread_mutexattr_t, pthread_rwlock_t,
    pthread_rwlockattr_t, timespec,
};

use crate::errors::*;

/// 超时设置
pub enum Timeout {
    Infinite,
    Val(std::time::Duration),
}
/// 共享互斥锁的守护结构
pub struct SharedMutexGuard<'t, T> {
    lock: &'t SharedMutex<T>,
}
impl<'t, T> Drop for SharedMutexGuard<'t, T> {
    fn drop(&mut self) {
        self.lock.unlock().unwrap();
    }
}
impl<'t, T> SharedMutexGuard<'t, T> {
    fn new(lock: &'t SharedMutex<T>) -> Self {
        Self {
            lock,
        }
    }
}
impl<'t, T> Deref for SharedMutexGuard<'t, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.into_inner() }
    }
}
impl<'t, T> DerefMut for SharedMutexGuard<'t, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.into_inner() }
    }
}

/// 共享读写锁的读守护结构
pub struct SharedRwLockReadGuard<'t, T> {
    data: NonNull<T>,
    lock: &'t *mut pthread_rwlock_t,
}
impl<'t, T> Drop for SharedRwLockReadGuard<'t, T> {
    fn drop(&mut self) {
        println!("SharedRwLockReadGuard::drop called");
        unsafe { nix::libc::pthread_rwlock_unlock(*self.lock) };
    }
}
impl<'t, T> SharedRwLockReadGuard<'t, T> {
    fn new(lock: &'t SharedRwLock<T>) -> Self {
        Self {
            lock: &lock.ptr,
            data: unsafe { NonNull::new_unchecked(*lock.data.get()) },
        }
    }
}
impl<'t, T> Deref for SharedRwLockReadGuard<'t, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

/// 共享读写锁的写守护结构
pub struct SharedRwLockWriteGuard<'t, T> {
    lock: &'t SharedRwLock<T>,
}
impl<'t, T> Drop for SharedRwLockWriteGuard<'t, T> {
    fn drop(&mut self) {
        println!("SharedRwLockWriteGuard::drop called");
        self.lock.unlock().unwrap();
    }
}
impl<'t, T> SharedRwLockWriteGuard<'t, T> {
    fn new(lock: &'t SharedRwLock<T>) -> Self {
        Self {
            lock,
        }
    }
}
impl<'t, T> Deref for SharedRwLockWriteGuard<'t, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.into_inner() }
    }
}
impl<'t, T> DerefMut for SharedRwLockWriteGuard<'t, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.into_inner() }
    }
}
/// 用于进程间同步的共享条件变量结构
pub struct SharedCondVar {
    ptr: *mut pthread_cond_t,
    data: UnsafeCell<*mut u8>,
}

/// 用于进程间同步的共享互斥锁结构
pub struct SharedMutex<T> {
    ptr: *mut pthread_mutex_t,
    data: UnsafeCell<*mut T>,
}

impl<T> Drop for SharedMutex<T> {
    fn drop(&mut self) {
        unsafe {
            nix::libc::pthread_mutex_destroy(self.ptr);
        }
    }
}

impl<T> SharedMutex<T> {
    /// 在提供的缓冲区中初始化锁的新实例，并返回使用的字节数
    unsafe fn new(mem: *mut u8, data: T) -> Result<(Self, usize), MutexLockError> {
        unsafe {
            // 计算在当前内存地址 mem 之后，需要填充（padding）多少字节才能使接下来的数据对齐到指针 (*mut u8) 的边界上
            let padding = mem.align_offset(std::mem::size_of::<*mut u8>() as _);
            // 初始化互斥锁属性
            #[allow(invalid_value)]
            let mut lock_attr =
                std::mem::MaybeUninit::<pthread_mutexattr_t>::uninit().assume_init();
            // 初始化互斥锁属性
            match nix::libc::pthread_mutexattr_init(&mut lock_attr) {
                0 => {}
                err_code => {
                    return Err(MutexLockError::InitError(err_code));
                }
            }
            // 设置互斥锁属性为进程间共享
            match nix::libc::pthread_mutexattr_setpshared(&mut lock_attr, PTHREAD_PROCESS_SHARED) {
                0 => {}
                err_code => {
                    return Err(MutexLockError::InitError(err_code));
                }
            }
            // 计算互斥锁指针,移动对齐后的地址
            let ptr = mem.add(padding) as *mut pthread_mutex_t;
            // 初始化互斥锁
            match nix::libc::pthread_mutex_init(ptr, &lock_attr) {
                0 => {}
                err_code => {
                    return Err(MutexLockError::InitError(err_code));
                }
            }
            let data_ptr = mem.add(padding + std::mem::size_of::<pthread_mutex_t>()) as *mut T;
            std::ptr::write(data_ptr, data);
            let shared_mutex = Self {
                ptr,
                data: UnsafeCell::new(data_ptr),
            };
            Ok((
                shared_mutex,
                padding + std::mem::size_of::<pthread_mutex_t>() + std::mem::size_of::<T>(),
            ))
        }
    }

    /// 从已初始化的内存位置重用锁，并返回使用的字节数
    unsafe fn try_into(mem: *mut u8) -> (Self, usize) {
        unsafe {
            let padding = mem.align_offset(std::mem::size_of::<*mut u8>() as _);
            let ptr = mem.add(padding) as *mut pthread_mutex_t;
            let data_ptr = mem.add(padding + std::mem::size_of::<pthread_mutex_t>()) as *mut T;
            let shared_mutex = Self {
                ptr,
                data: UnsafeCell::new(data_ptr),
            };
            (
                shared_mutex,
                padding + std::mem::size_of::<pthread_mutex_t>() + std::mem::size_of::<T>(),
            )
        }
    }

    fn as_raw(&self) -> *mut std::ffi::c_void {
        self.ptr as *mut std::ffi::c_void
    }

    /// Acquires the lock
    fn lock(&self) -> Result<SharedMutexGuard<'_, T>, MutexLockError> {
        unsafe {
            match nix::libc::pthread_mutex_lock(self.ptr) {
                0 => Ok(SharedMutexGuard::new(self)),
                err_code => Err(MutexLockError::LockError(err_code)),
            }
        }
    }

    fn try_lock(&self) -> Result<SharedMutexGuard<'_, T>, MutexLockError> {
        unsafe {
            match nix::libc::pthread_mutex_trylock(self.ptr) {
                0 => Ok(SharedMutexGuard::new(self)),
                err_code => Err(MutexLockError::TryLockError(err_code)),
            }
        }
    }
    /// Acquires lock with timeout
    fn time_lock(&self, timeout: Timeout) -> Result<SharedMutexGuard<'_, T>, MutexLockError> {
        // For simplicity, we ignore timeout and just try to lock
        let timespec = match timeout {
            Timeout::Infinite => return self.lock(),
            Timeout::Val(dur) => {
                // 计算超时时间点
                let now = std::time::SystemTime::now();
                let cur_time = now + dur;
                let since_epoch = cur_time.duration_since(std::time::UNIX_EPOCH).unwrap();
                timespec {
                    tv_sec: since_epoch.as_secs() as _,
                    tv_nsec: since_epoch.subsec_nanos() as _,
                }
            }
        };
        unsafe {
            match nix::libc::pthread_mutex_timedlock(self.ptr, &timespec) {
                0 => Ok(SharedMutexGuard::new(self)),
                err_code => Err(MutexLockError::TimeoutError(err_code)),
            }
        }
    }

    /// Release the lock
    fn unlock(&self) -> Result<(), MutexLockError> {
        unsafe {
            match nix::libc::pthread_mutex_unlock(self.ptr) {
                0 => Ok(()),
                err_code => Err(MutexLockError::UnlockError(err_code)),
            }
        }
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn into_inner(&self) -> *mut T {
        unsafe { *self.data.get() }
    }
}

/// 用于进程间同步的共享读写锁结构
pub struct SharedRwLock<T> {
    ptr: *mut pthread_rwlock_t,
    data: UnsafeCell<*mut T>,
}

impl<T> Drop for SharedRwLock<T> {
    fn drop(&mut self) {
        println!("SharedRwLock::drop called");
        unsafe {
            nix::libc::pthread_rwlock_destroy(self.ptr);
        }
    }
}

impl<T> SharedRwLock<T> {
    pub fn new(mem: *mut u8, data: T) -> Result<(Self, usize), RwLockError> {
        unsafe {
            let padding = mem.align_offset(std::mem::size_of::<*mut u8>() as _);
            #[allow(invalid_value)]
            let mut lock_attr =
                std::mem::MaybeUninit::<pthread_rwlockattr_t>::uninit().assume_init();
            // 初始化读写锁属性
            match nix::libc::pthread_rwlockattr_init(&mut lock_attr) {
                0 => {}
                err_code => {
                    return Err(RwLockError::InitError(err_code));
                }
            }
            // 设置读写锁属性为进程间共享
            match nix::libc::pthread_rwlockattr_setpshared(&mut lock_attr, PTHREAD_PROCESS_SHARED) {
                0 => {}
                err_code => {
                    return Err(RwLockError::InitError(err_code));
                }
            }
            // 计算读写锁指针,移动对齐后的地址
            let ptr = mem.add(padding) as *mut pthread_rwlock_t;
            match nix::libc::pthread_rwlock_init(ptr, &lock_attr) {
                0 => {}
                err_code => {
                    return Err(RwLockError::InitError(err_code));
                }
            }
            // 写入数据到共享内存
            let data_ptr = mem.add(padding + std::mem::size_of::<pthread_rwlock_t>()) as *mut T;
            std::ptr::write(data_ptr, data);
            let shared_rwlock = Self {
                ptr,
                data: UnsafeCell::new(data_ptr),
            };
            Ok((
                shared_rwlock,
                padding + std::mem::size_of::<pthread_rwlock_t>() + std::mem::size_of::<T>(),
            ))
        }
    }

    pub fn try_into(mem: *mut u8) -> Result<(Self, usize), RwLockError> {
        unsafe {
            let padding = mem.align_offset(std::mem::size_of::<*mut u8>() as _);
            let ptr = mem.add(padding) as *mut pthread_rwlock_t;
            let data_ptr = mem.add(padding + std::mem::size_of::<pthread_rwlock_t>()) as *mut T;
            let shared_rwlock = Self {
                ptr,
                data: UnsafeCell::new(data_ptr),
            };
            //TODO: 检查指针有效性
            if ptr.is_null() || data_ptr.is_null() {
                return Err(RwLockError::IntoError);
            }
            Ok((
                shared_rwlock,
                padding + std::mem::size_of::<pthread_rwlock_t>() + std::mem::size_of::<T>(),
            ))
        }
    }

    fn as_raw(&self) -> *mut std::ffi::c_void {
        self.ptr as *mut std::ffi::c_void
    }

    pub fn read(&self) -> Result<SharedRwLockReadGuard<'_, T>, RwLockError> {
        unsafe {
            match nix::libc::pthread_rwlock_rdlock(self.ptr) {
                0 => Ok(SharedRwLockReadGuard {
                    lock: &self.ptr,
                    data: NonNull::new_unchecked(*self.data.get()),
                }),
                err_code => Err(RwLockError::ReadLockError(err_code)),
            }
        }
    }

    fn try_read(&self) -> Result<SharedRwLockReadGuard<'_, T>, RwLockError> {
        unsafe {
            match nix::libc::pthread_rwlock_tryrdlock(self.ptr) {
                0 => Ok(SharedRwLockReadGuard {
                    lock: &self.ptr,
                    data: NonNull::new_unchecked(*self.data.get()),
                }),
                err_code => Err(RwLockError::TryReadLockError(err_code)),
            }
        }
    }

    pub fn write(&self) -> Result<SharedRwLockWriteGuard<'_, T>, RwLockError> {
        unsafe {
            match nix::libc::pthread_rwlock_wrlock(self.ptr) {
                0 => Ok(SharedRwLockWriteGuard {
                    lock: self,
                }),
                err_code => Err(RwLockError::WriteLockError(err_code)),
            }
        }
    }

    fn try_write(&self) -> Result<SharedRwLockWriteGuard<'_, T>, RwLockError> {
        unsafe {
            match nix::libc::pthread_rwlock_trywrlock(self.ptr) {
                0 => Ok(SharedRwLockWriteGuard {
                    lock: self,
                }),
                err_code => Err(RwLockError::TryWriteLockError(err_code)),
            }
        }
    }

    fn unlock(&self) -> Result<(), RwLockError> {
        unsafe {
            match nix::libc::pthread_rwlock_unlock(self.ptr) {
                0 => Ok(()),
                err_code => Err(RwLockError::WriteUnlockError(err_code)),
            }
        }
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn into_inner(&self) -> *mut T {
        unsafe { *self.data.get() }
    }
}
