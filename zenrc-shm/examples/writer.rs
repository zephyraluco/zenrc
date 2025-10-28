use std::slice;

use arrow::array::{Array, Int32Array};
use zenrc_shm::shm::MemoryHandle;

fn main() {
    let name = "/my_shared_mem";
    let size: usize = 4096; // 4KB

    let mut mem_handle = MemoryHandle::new(name, size).expect("MemoryHandle::new failed");
    // 创建一个 Arrow 数组
    let arr = Int32Array::from(vec![10, 20, 30, 40]);
    let data = arr.to_data();
    let buffer = data.buffers()[0].as_slice();
    let len = buffer.len();

    // 拷贝数据到共享内存
    unsafe {
        let slice = slice::from_raw_parts_mut(mem_handle.get_mut_ptr().as_ptr(), len);
        slice.copy_from_slice(buffer);
    }

    println!("✅ Written to shared memory: {:?}", arr);
    println!("写入的字节数: {}", buffer.len());
    println!("内容十六进制: {:02X?}", buffer);

    // 程序退出前清理共享内存对象
    // 'l: loop {
    //     std::thread::sleep(std::time::Duration::from_secs(1));
    // }
    println!("Exiting and cleaning up shared memory.");
    // 共享内存对象会在 MemoryHandle 的 Drop 实现中被清理
}
