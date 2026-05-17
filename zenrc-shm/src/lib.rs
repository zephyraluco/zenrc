//! # zenrc-shm
//!
//! 基于 POSIX 共享内存的进程间通信原语。
//!
//! 提供了以下核心组件：
//!
//! - [`shm::MemoryHandle`]：POSIX 共享内存段的 RAII 句柄，自动处理创建、映射与释放。
//! - [`ringbuffer::MpmcRingBuffer`]：构建于共享内存上的多生产者/多消费者无锁环形缓冲区。
//! - [`sync`]：进程间共享的互斥锁（[`sync::SharedMutex`]）与读写锁（[`sync::SharedRwLock`]）。
//! - [`errors`]：各同步原语的错误类型。
//!
//! ## 示例
//!
//! ```no_run
//! use zenrc_shm::shm::MemoryHandle;
//!
//! // 创建或打开一块 4096 字节的共享内存段
//! let mut mem = MemoryHandle::new("/my_shm", 4096).unwrap();
//! ```

pub mod shm;
pub mod sync;
pub mod errors;
pub mod ringbuffer;
