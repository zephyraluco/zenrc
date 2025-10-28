use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zenrc_shm::shm::MemoryHandle;
use zenrc_shm::ringbuffer::MpmcRingBuffer;



fn main() {
    // 注册一个原子布尔标志，用于检测 Ctrl+C
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // 注册 Ctrl+C 处理器
    ctrlc::set_handler(move || {
        eprintln!("\nCtrl+C detected, shutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // 初始化共享内存
    let name = "/my_shared_mem";
    let size: usize = 4096;
    let mut data = 1;

    let mut mem_handle = MemoryHandle::new(name, size).expect("MemoryHandle::new failed");
    let ring_buffer = MpmcRingBuffer::<i32>::new(mem_handle.get_mut_ptr().as_ptr(), 10);

    // 主循环
    while running.load(Ordering::SeqCst) {
        ring_buffer.write(data);
        println!("Wrote value to shared memory: {}", data);
        data += 1;
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // 退出前自动释放资源（MemoryHandle drop）
    println!("Gracefully exiting...");
}
