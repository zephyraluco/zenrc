use std::cell::Cell;
use std::sync::atomic::AtomicUsize;

use crate::errors;
use crate::shm::MemoryHandle;
use crate::sync::SharedRwLock;

pub struct MpmcRingBuffer<T> {
    buffer: Vec<SharedRwLock<T>>,
    capacity: *mut usize,
    write_seq: *mut AtomicUsize,
    read_seq: Cell<usize>,
}

impl<T: Default> MpmcRingBuffer<T> {
    pub fn new(
        mem_handle: &mut MemoryHandle,
        capacity: usize,
    ) -> Result<Self, errors::RwLockError> {
        unsafe {
            if !mem_handle.is_owner() {
                return MpmcRingBuffer::<T>::try_into(mem_handle.get_mut_ptr().as_ptr());
            }
            let mem = mem_handle.get_mut_ptr().as_ptr();
            let padding = mem.align_offset(std::mem::size_of::<*mut u8>() as _);
            let cap_ptr = mem.add(padding) as *mut usize;
            std::ptr::write(cap_ptr, capacity);
            let seq_ptr = mem.add(padding + std::mem::size_of::<usize>()) as *mut AtomicUsize;
            std::ptr::write(seq_ptr, AtomicUsize::new(0));
            let mut buffer = Vec::with_capacity(capacity);
            let mut ptr = mem
                .add(std::mem::size_of::<usize>() + std::mem::size_of::<AtomicUsize>() + padding);
            for _ in 0..capacity {
                let slot_padding = ptr.align_offset(std::mem::size_of::<*mut u8>() as _);
                let (slot, size) =
                    SharedRwLock::<T>::new(ptr.add(slot_padding), T::default()).unwrap();
                buffer.push(slot);
                ptr = ptr.add(size + slot_padding);
            }

            Ok(Self {
                buffer,
                capacity: cap_ptr,
                write_seq: seq_ptr,
                read_seq: Cell::new(0),
            })
        }
    }

    pub fn write(&self, value: T) {
        let write_seq =
            unsafe { (*self.write_seq).fetch_add(1, std::sync::atomic::Ordering::SeqCst) };
        let index = write_seq % unsafe { *self.capacity };
        println!("Writing at write_seq: {}", write_seq);
        let mut guard = self.buffer[index].write().unwrap();
        *guard = value;
    }

    pub fn read(&self) -> Result<T, errors::RwLockError>
    where
        T: Copy,
    {
        println!("Current read_seq: {}", self.read_seq.get());
        let seq = unsafe { (*self.write_seq).load(std::sync::atomic::Ordering::SeqCst) };
        if self.read_seq.get() == 0 {
            self.read_seq.set(seq);
        } else if self.read_seq.get() < seq {
            self.read_seq.set(self.read_seq.get() + 1);
        } else {
            return Err(errors::RwLockError::Empty);
        }
        let index = (self.read_seq.get() - 1) % unsafe { *self.capacity };
        let guard = self.buffer[index].read().unwrap();
        Ok(*guard)
    }

    pub fn try_into(mem: *mut u8) -> Result<Self, errors::RwLockError> {
        unsafe {
            let padding = mem.align_offset(std::mem::size_of::<*mut u8>() as _);
            let cap_ptr = mem.add(padding) as *mut usize;
            let capacity = *cap_ptr;
            let seq_ptr = mem.add(padding + std::mem::size_of::<usize>()) as *mut AtomicUsize;
            let mut buffer = Vec::with_capacity(capacity);
            let mut ptr = mem
                .add(std::mem::size_of::<usize>() + std::mem::size_of::<AtomicUsize>() + padding);
            for _ in 0..capacity {
                let slot_padding: usize = ptr.align_offset(std::mem::size_of::<*mut u8>() as _);
                let (slot, size) = SharedRwLock::<T>::try_into(ptr.add(slot_padding)).unwrap();
                buffer.push(slot);
                ptr = ptr.add(size + slot_padding);
            }
            //TODO: 检查指针有效性
            if cap_ptr.is_null() || ptr.is_null() {
                return Err(errors::RwLockError::IntoError);
            }
            Ok(Self {
                buffer,
                capacity: cap_ptr,
                write_seq: seq_ptr,
                read_seq: Cell::new(0),
            })
        }
    }
}
