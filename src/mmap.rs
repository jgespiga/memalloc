use std::{alloc::Layout, os::raw::{c_int, c_void}, ptr};

use libc::{mmap, munmap, off_t, size_t};


struct MmapAllocator;

impl MmapAllocator {
    #[inline]
    pub const fn new() -> Self {
        Self
    }

    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            println!("{}", libc::sysconf(libc::_SC_PAGE_SIZE));
        }
        const ADDR: *mut c_void = ptr::null_mut::<c_void>();
        let length = layout.size() as size_t;
        const PROT: c_int = libc::PROT_READ | libc::PROT_WRITE;

        const FLAGS: c_int = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
        const FD: c_int = -1;
        const OFFSET: off_t = 0;

        unsafe {
            match mmap(ADDR, length, PROT, FLAGS, FD, OFFSET) {
                libc::MAP_FAILED => ptr::null_mut::<u8>(),
                address => address as *mut u8
                
            }
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let addr = ptr as *mut c_void;
        let length = layout.size() as size_t;
        unsafe { munmap(addr, length); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_allocation_mmap() {
        let allocator = MmapAllocator::new();

        unsafe {
            let layout = Layout::new::<u32>();
            // Allocated space for unsigned 32 bit integer.
            let block1 = allocator.alloc(layout);
            let block2 = allocator.alloc(layout);
            println!("{:?}", block1);
            println!("{:?}", block2);

            *block1 = 2;
            assert_eq!(*block1, 2);

            *block2 = 45;
            assert_eq!(*block2, 45);
        }
    }

}