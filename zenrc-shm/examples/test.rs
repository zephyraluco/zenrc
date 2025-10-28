use zenrc_shm::shm::MemoryHandle;
use zenrc_shm::sync::SharedRwLock;

fn main() {
    let name = "/my_shared_mem";
    let size: usize = 4096; // 4KB

    let mut mem_handle = MemoryHandle::new(name, size).expect("MemoryHandle::new failed");
    let (a, b) = SharedRwLock::new(mem_handle.get_mut_ptr().as_ptr(), 101).unwrap();
    let mut guard = a.write().unwrap();
    *guard = 123;

    let value = *guard;
    // let mut value = *guard;
    // unsafe {
        // std::ptr::write(*guard,234);
        // value = std::ptr::read( *guard);
    // };
    std::thread::sleep(std::time::Duration::from_secs(1));
    println!("Read value from shared memory: {:?}", value);
}
