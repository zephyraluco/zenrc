use std::sync::atomic::AtomicUsize;

use crate::errors;
use crate::sync::SharedRwLock;

pub struct MpmcRingBuffer<T> {
    buffer: Vec<SharedRwLock<T>>,
    capacity: *mut usize,
    seq: *mut AtomicUsize,
}

impl<T: Default> MpmcRingBuffer<T> {
    pub fn new(mem: *mut u8, capacity: usize) -> Self {
        unsafe {
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

            Self {
                buffer,
                capacity: cap_ptr,
                seq: seq_ptr,
            }
        }
    }

    pub fn write(&self, value: T) {
        let seq = unsafe { (*self.seq).fetch_add(1, std::sync::atomic::Ordering::SeqCst) };
        let index = seq % unsafe { *self.capacity };
        let mut guard = self.buffer[index].write().unwrap();
        *guard = value;
    }

    pub fn read(&self) -> T
    where
        T: Copy,
    {
        let seq = unsafe { (*self.seq).load(std::sync::atomic::Ordering::SeqCst) };
        let index = (seq - 1) % unsafe { *self.capacity };
        let guard = self.buffer[index].read().unwrap();
        *guard
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
                seq: seq_ptr,
            })
        }
    }
}
