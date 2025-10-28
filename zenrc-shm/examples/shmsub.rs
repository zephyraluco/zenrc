use zenrc_shm::ringbuffer::MpmcRingBuffer;
use zenrc_shm::shm::MemoryHandle;

fn main() {
    let name = "/my_shared_mem";
    // let size: usize = 4096; // 4KB

    let mut mem_handle = MemoryHandle::open(name).expect("MemoryHandle::new failed");
    let ring_buffer = MpmcRingBuffer::<i32>::try_into(mem_handle.get_mut_ptr().as_ptr()).unwrap();
    loop {
        let value = ring_buffer.read();
        println!("Read value from shared memory: {}", value);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
