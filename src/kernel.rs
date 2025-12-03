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
            PAGE_SIZE = Kernel::page_size();
        }

        PAGE_SIZE
    }
}


trait PlatformMemory {
    unsafe fn request_memory(len: usize) -> Option<NonNull<u8>>;

    unsafe fn return_memory(addr: *mut u8, len: usize);

    unsafe fn page_size() -> usize;
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



/// Wrapper to use [`Kernel::request_memory`] 
#[inline]
pub(crate) unsafe fn request_memory(len: usize) -> Option<NonNull<u8>> {
    unsafe { Kernel::request_memory(len) }
} 

/// Wrapper to use [`Kernel::return_memory`]
#[inline]
pub(crate) unsafe fn return_memory(addr: *mut u8, len: usize) {
    unsafe { Kernel::return_memory(addr, len); }
}

#[cfg(unix)]
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

        unsafe fn return_memory(addr: *mut u8, len: usize) {
            unsafe { munmap(addr as *mut c_void, len as size_t); }
        }

        unsafe fn page_size() -> usize {
            unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) as usize }
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::{mem::MaybeUninit, ptr::NonNull, os::raw::c_void};

    use crate::kernel::{Kernel, PlatformMemory};

    use windows::Win32::System::{Memory, SystemInformation};

    impl PlatformMemory for Kernel {
        unsafe fn request_memory(len: usize) -> Option<std::ptr::NonNull<u8>> {
            // Read-Write only.
            let protection = Memory::PAGE_READWRITE;

            let flags = Memory::MEM_RESERVE | Memory::MEM_COMMIT;

            unsafe {
                let addr = Memory::VirtualAlloc(None, len, flags, protection);
                
                NonNull::new(addr.cast())
            }
        }

        unsafe fn return_memory(addr: *mut u8, _len: usize) {
            unsafe { Memory::VirtualFree(addr as *mut c_void, 0, Memory::MEM_RELEASE); }
        }

        unsafe fn page_size() -> usize {
            unsafe {
                let mut system_info = MaybeUninit::uninit();
                SystemInformation::GetSystemInfo(system_info.as_mut_ptr());
                
                system_info.assume_init().dwPageSize as usize
            }
        }
    }
}