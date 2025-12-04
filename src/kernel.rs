use std::{alloc::Layout, ptr::NonNull};
use crate::{block::{BLOCK_HEADER_SIZE, Block}, freelist::FreeList, list::List, mmap::MIN_BLOCK_SIZE, region::{REGION_HEADER_SIZE, Region}, utils::align};

/// Virtual memory page siz of the computer. This is usually 4096.
/// This value should be a constant, but we can't do that since we 
/// don't know the value at compile time.
pub(crate) static mut PAGE_SIZE: usize = 0;

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

/// This trait provides an abstraction to handle low level memory operations
/// and syscalls. As the allocator, our top level view of this, has nothing
/// to do with the concrete implementations / APIs offered by each kernel.
trait PlatformMemory {
    /// Request a memory region of size `len`. It returns a Pointer to the 
    /// given location or None if the underlying syscall fails.
    unsafe fn request_memory(len: usize) -> Option<NonNull<u8>>;

    /// Returns the memory of size `len` starting from `addr` back to the kernel.
    unsafe fn return_memory(addr: *mut u8, len: usize);

    /// Returns the virtual memory page size of the computer in bytes.
    unsafe fn page_size() -> usize;
}


impl Kernel {
    /// Create a new instance of the allocator's `Kernel`. 
    /// 
    /// When created, it will calculate the computer's page size and 
    /// initialize both the free list and the regions list to be 
    /// new empty [`FreeList`] and [`List`] datastructures.
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

/// Wrapper to calculate the computer's page size.
#[inline]
pub(crate) fn page_size() -> usize {
    unsafe {
        if PAGE_SIZE == 0 {
            PAGE_SIZE = Kernel::page_size();
        }

        PAGE_SIZE
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
            // mmap parameters.
            const ADDR: *mut c_void = std::ptr::null_mut::<c_void>();
            // Read-Write only memory.
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

impl Kernel {
    /// This function returns a new memory `region` by using [`request_memory`].
    /// 
    /// If we don't have any free block we can use on our free list, we know for
    /// sure there is no way we can allocate the requested size on our current
    /// Regions. Therefor, we need to allocate a new [`Region`] using
    /// [`libc::mmap`].
    /// 
    /// This implementation is platform-dependant. It only works on linux right now.
    pub fn allocate_new_region(&mut self, layout: Layout) -> Result<(), &'static str> {

        // What we really need to allocate is the requested size (aligned)
        // plus the overhead introduced by out allocator's data structures
        let layout_size = align(layout.size(), std::mem::size_of::<usize>());

        // The minimun block size we can give to the user is `MIN_BLOCK_SIZE`. If we
        // didn't do this, we wouldn't be able to store our allocator's metadata on
        // small memory requests.
        let needed_payload = std::cmp::max(layout_size, MIN_BLOCK_SIZE);

        let needed = needed_payload + BLOCK_HEADER_SIZE;

        let region_size = align(needed, self.page_size);

        unsafe {    
            // What should we do here? I assume its okay to panic if 
            // we get None from calling `mmap`.
            let addr = request_memory(region_size).expect("mmap syscall returned None");

            let mut region = self.regions.append(
                Region {
                    size: region_size - REGION_HEADER_SIZE,
                    blocks: List::new(),
                },

                addr
            );

            // First Node<Block> right after Node<Region>
            let block_addr = NonNull::new_unchecked(region.as_ptr().offset(1)).cast();

            // Useful block size
            let block_size = region.as_ref().data.size - BLOCK_HEADER_SIZE;

            let block = region.as_mut().data.blocks.append(
                Block {
                    size: block_size,
                    is_free: true,
                    region,
                },
                block_addr,
            );

            // We use the payload of the free block to store the node
            let free_node_addr = NonNull::new_unchecked(
                block.as_ptr().cast::<u8>().add(BLOCK_HEADER_SIZE)
            );

            self.free_list.insert_free_block(block, free_node_addr);
        }
        
        Ok(())
    }


}