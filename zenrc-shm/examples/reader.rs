use std::ffi::CString;
use std::num::NonZeroUsize;
use std::ptr::NonNull;
use std::sync::Arc;

use arrow::alloc::Allocation;
use arrow::array::{ArrayData, Int32Array};
use arrow::buffer::Buffer;
use arrow::datatypes::DataType;
use nix::fcntl::OFlag;
use nix::sys::mman::{MapFlags, ProtFlags, mmap, shm_open};
use zenrc_shm::shm::MemoryHandle;

// 不需要实现 Allocation
#[derive(Debug)]
struct SharedMemAlloc;

fn main() {
    let name = "/my_shared_mem";

    let mut mem_handle = MemoryHandle::open(name).unwrap();
    let len = 4; // 数组元素个数
    let byte_len = len * std::mem::size_of::<i32>();

    // 将 mmap 封装为 Arrow Buffer（零拷贝）
    let ptr = NonNull::new(mem_handle.get_mut_ptr().as_ptr()).unwrap();
    let buffer = unsafe { Buffer::from_custom_allocation(ptr, byte_len, Arc::new(SharedMemAlloc)) };

    // ✅ 用 ArrayData 构造 Int32Array
    let array_data = ArrayData::try_new(
        DataType::Int32, // 数据类型
        len,             // 元素个数
        None,            // null bitmap（这里没有 null）
        0,            // offset（偏移量）
        vec![buffer], // 数据缓冲区（一个或多个）
        vec![],       // 子数组（struct/list 用，这里为空）
    )
    .unwrap();

    let arr = Int32Array::from(array_data);

    println!("✅ Reader: read Int32Array = {:?}", arr);
}
