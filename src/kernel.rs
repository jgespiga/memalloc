use std::ptr::NonNull;
use crate::{freelist::FreeList, list::List, region::Region};

/// Virtual memory page siz of the computer. This is usually 4096.
/// This value should be a constant, but we can't do that since we 
/// don't know the value at compile time.
pub(crate) static mut PAGE_SIZE: usize = 0;

#[inline]
pub(crate) fn page_size() -> usize {
    unsafe {
        if PAGE_SIZE == 0 {
            PAGE_SIZE = libc::sysconf(libc::_SC_PAGE_SIZE) as usize;
        }

        PAGE_SIZE
    }
}
/// The internal data structure of the allocator. Here is where
/// we manage the low level memory request as well as platform-dependant
/// stuff.
pub(crate) struct Kernel {
    /// Linked list of allocator memory [`Region`]
    pub regions: List<Region>,
    /// Computer's page size (used for aligment). See [`MmapAllocator::align`]
    pub page_size: usize,
    /// Linked list of free blocks identified by [`Block::is_free`]
    pub free_list: FreeList,
}

impl Kernel {
    pub(crate) fn new() -> Self {
        page_size();
        unsafe {
            Self {
                regions: List::new(),
                page_size: PAGE_SIZE, 
                free_list: FreeList::new()
            }
        }
    }
}


trait PlatformMemory {
    unsafe fn request_memory(len: usize) -> Option<NonNull<u8>>;

    unsafe fn return_memory(address: NonNull<u8>, len: usize);
}

#[inline]
pub(crate) unsafe fn request_memory(len: usize) -> Option<NonNull<u8>> {
    unsafe { Kernel::request_memory(len) }
} 

#[cfg(target_os = "linux")]
mod unix {
    use super::{PlatformMemory, Kernel};

    use libc::{mmap, munmap, off_t, size_t};

    use std::{os::raw::{c_void, c_int}, ptr::{NonNull}};

    impl PlatformMemory for Kernel {
        unsafe fn request_memory(len: usize) -> Option<NonNull<u8>> {
            const ADDR: *mut c_void = std::ptr::null_mut::<c_void>();
            const PROT: c_int = libc::PROT_READ | libc::PROT_WRITE;
            const FLAGS: c_int = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
            const FD: c_int = -1;
            const OFFSET: off_t = 0;

            unsafe {    
                let addr = mmap(ADDR, len as size_t, PROT, FLAGS, FD, OFFSET);

                match addr {
                    libc::MAP_FAILED => None,
                    addr => Some(NonNull::new_unchecked(addr).cast::<u8>()),
                }
            }
        }

        unsafe fn return_memory(address: std::ptr::NonNull<u8>, len: usize) {
        
        }
    }
}

mod windows {
     
}