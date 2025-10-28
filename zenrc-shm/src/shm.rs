use std::num::NonZeroUsize;
use std::os::fd::{IntoRawFd, RawFd};
use std::ptr::NonNull;

use nix::fcntl::OFlag;
use nix::sys::mman::{MapFlags, ProtFlags, mmap, munmap, shm_open, shm_unlink};
use nix::sys::stat::fstat;
use nix::unistd::{close, ftruncate};

pub struct MemoryHandle {
    fd: RawFd,
    name: String,
    owner: bool,
    size: NonZeroUsize,
    ptr: NonNull<u8>,
}
impl Drop for MemoryHandle {
    fn drop(&mut self) {
        //解除内存映射
        if let Err(e) = unsafe { munmap(self.ptr.cast(), self.size.get()) } {
            eprintln!("Failed to unmap memory! {}", e);
        }

        if self.fd != 0 {
            //释放内存
            if self.owner {
                if let Err(err) = shm_unlink(self.name.as_str()) {
                    eprintln!("Failed to unlink shared memory: {}", err);
                }
            }
            if let Err(err) = close(self.fd) {
                eprintln!("Failed to close file descriptor: {}", err);
            }
        }
    }
}

impl MemoryHandle {
    pub fn new<T: Into<String>>(name: T, size: usize) -> Result<Self, std::io::Error> {
		let name= name.into();
        let fd = shm_open(
            name.as_str(),
            OFlag::O_CREAT | OFlag::O_RDWR, //创建并可读写
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR, //主有者可读写
        )?;
        //设置共享内存大小
        ftruncate(&fd, size as i64)?;
        let nz_size = NonZeroUsize::new(size).unwrap();
        let ptr = unsafe {
            //映射到进程的虚拟内存
            mmap(
                None, //为NULL，表示由系统选择映射地址
                nz_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE, //可读可写
                MapFlags::MAP_SHARED,                         //共享映射
                &fd,                                          //文件描述符
                0,
            )?
        };

        Ok(Self {
            fd: fd.into_raw_fd(),
            name,
            owner: true,
            size: nz_size,
            ptr: ptr.cast(),
        })
    }

    pub fn open<T: Into<String>>(name: T,) -> Result<Self, std::io::Error> {
		let name= name.into();
        let fd = shm_open(
            name.as_str(),
            OFlag::O_RDWR,                                                 //可读写
            nix::sys::stat::Mode::S_IRUSR, //主有者可读
        )?;
		let size = fstat(&fd).unwrap().st_size as usize;
        let nz_size = NonZeroUsize::new(size).unwrap();
        let ptr = unsafe {
            //映射到进程的虚拟内存
            mmap(
                None, //为NULL，表示由系统选择映射地址
                nz_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE, //可读可写
                MapFlags::MAP_SHARED,                         //共享映射
                &fd,                                          //文件描述符
                0,
            )?
        };

        Ok(Self {
            fd: fd.into_raw_fd(),
            name,
            owner: false,
            size: nz_size,
            ptr: ptr.cast(),
        })
    }
    pub fn get_mut_ptr(&mut self) -> NonNull<u8> {
        self.ptr
    }
    pub fn set_owner(&mut self, owner: bool) {
        self.owner = owner;
    }
}
