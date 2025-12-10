use std::{alloc::Layout, f32::MIN, mem, ptr::NonNull};
use crate::{block::{BLOCK_HEADER_SIZE, Block}, freelist::FreeList, list::{List, Node}, memalloc::MIN_BLOCK_SIZE, region::{REGION_HEADER_SIZE, Region}, utils::align};

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
    /// Computer's page size (used for aligment). See [`MemAlloc::align`]
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
        /// Request a raw chunk of memory from the operating system using `mmap`.
        /// 
        /// This function requests a new memory mapping that is:
        /// - Readable and Writable
        /// - Anonymous
        /// - Private
        /// 
        /// # Arguments
        /// 
        /// `len` - The size of the memory region to request in bytes.
        /// 
        /// # Safety
        /// 
        /// It performs a raw system call. The returned memory is uninitialized.
        unsafe fn request_memory(len: usize) -> Option<NonNull<u8>> {
            // mmap parameters
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

        /// Releases a previously allocated memory segment back to the operating system.
        /// 
        /// This function wraps the `munmap` system call.
        /// 
        /// # Safety
        /// 
        /// The caller must ensure that:
        /// - `addr` is a valid pointer previously returned by `request_memory`
        /// - `len` matches the size of the mapping to be unmapped
        /// - The memory at `addr` is not accessed after this call (Which will result in Use-After-Free errors)
        unsafe fn return_memory(addr: *mut u8, len: usize) {
            unsafe { munmap(addr as *mut c_void, len as size_t); }
        }

        /// Returns the system's virtual memory page size in bytes.
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
        /// Requests memory from the Windows Operating System.
        /// 
        /// This implementation uses `VirtualAlloc` to reserve and commit memory
        /// in a single step.
        /// 
        /// # Arguments
        /// 
        /// - `len` - The number of bytes to allocate.
        unsafe fn request_memory(len: usize) -> Option<std::ptr::NonNull<u8>> {
            // Read-Write only.
            let protection = Memory::PAGE_READWRITE;
            
            // Reserve address space and commit physical storage immediately.
            let flags = Memory::MEM_RESERVE | Memory::MEM_COMMIT;

            unsafe {
                let addr = Memory::VirtualAlloc(None, len, flags, protection);
                
                NonNull::new(addr.cast())
            }
        }

        /// Release a memory region previously allocated by `VirtualAlloc`.
        /// 
        /// This function wraps `Virtuall`.
        /// 
        /// # Windows Specific Behavior
        ///
        /// According to the Microsoft documentation for `VirtualFree` with `MEM_RELEASE`:
        /// 
        /// - "If the dwFreeType parameter is MEM_RELEASE, this parameter [dwSize] 
        /// - must be 0 (zero). The function frees the entire region that is reserved 
        /// - in the initial allocation call to VirtualAlloc."
        /// 
        /// Therefore, `_len` is ignored to prevent `VirtualFree` from failing.
        ///
        /// # Safety
        ///
        /// Caller must ensure `addr` is a valid pointer returned by `request_memory`
        /// and has not been freed yet.
        unsafe fn return_memory(addr: *mut u8, _len: usize) {
            unsafe { let _ = Memory::VirtualFree(addr as *mut c_void, 0, Memory::MEM_RELEASE); }
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

unsafe impl Send for Kernel {}
unsafe impl Sync for Kernel {}

impl Kernel {
    /// Create a new instance of the allocator's `Kernel`. 
    /// 
    /// When created, it will calculate the computer's page size and 
    /// initialize both the free list and the regions list to be 
    /// new empty [`FreeList`] and [`List`] datastructures.
    /// 
    /// We set the page_size to 0 in order to be able to make this constructor `const`.
    /// We will set the page_size later in [`Kernel::allocate_new_region`]
    pub(crate) const fn new() -> Self {
        Self {
            regions: List::new(),
            page_size: 0, 
            free_list: FreeList::new()
        }
    }

    
    /// This function returns a new memory `region` by using [`request_memory`].
    /// 
    /// If we don't have any free block we can use on our free list, we know for
    /// sure there is no way we can allocate the requested size on our current
    /// Regions. Therefor, we need to allocate a new [`Region`] using
    /// [`libc::mmap`].
    /// 
    /// This implementation is platform-dependant. It only works on linux right now.
    pub(crate) fn allocate_new_region(&mut self, layout: Layout) -> Result<(), &'static str> {

        if self.page_size == 0 {
            page_size();
            unsafe { self.page_size = PAGE_SIZE; }
        }

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

    /// Checks if the given `region` needs to be returned to the OS or not.
    ///  
    /// It manages the free_list and the state of `block` which might be the only block left in the region.
    /// We need this `block` since, if we were to munmap this region, we also need to remove that block
    /// from out free_list to avoid future problems. If we didn't do that, our allocator could think that
    /// this `block` stills free and therefor it will try to use it, causing undefined behavior.
    pub(crate) fn check_region_removal(&mut self, region: &mut NonNull<Node<Region>>, block: NonNull<Node<Block>>) {
        unsafe {
            if region.as_mut().data.blocks.len() == 1 {
                let total_region_size = region.as_ref().data.size + REGION_HEADER_SIZE;
                
                // Just in case the block stills in the free list, we always remove it.
                // If it was not in the free list, `remove_free_block` will manage it
                self.free_list.remove_free_block(block);
                self.regions.remove(*region);
                
                let region_start = region.as_ptr() as *mut u8;

                return_memory(region_start, total_region_size);
            } else {
                // The current region still has other blocks so the merged block has to return to the free list.
                
                // We remove the block from the list, and we reinsert it with the correct size.
                self.free_list.remove_free_block(block);
                // We use the free block payload
                let free_block_addr = 
                NonNull::new_unchecked((block.as_ptr() as *mut u8)
                .add(BLOCK_HEADER_SIZE));
            
                self.free_list.insert_free_block(block, free_block_addr);
            }
        }
    }

    /// Splits the given `block` if possible
    /// 
    /// ```text
    /// 
    ///  +----------------> Given addr                                          
    ///  |                                                                                   +-----> Returned addr
    ///  |          +-----> Start of the block                                               |
    ///  |          |                                                                        |
    ///  +-------------------------------------------------+                      +----------+---------+---------+------------------+ 
    ///  |          |                                      |      We split it     |          |         |         |                  |
    ///  |  Header  |            Free Block                | -------------------> |  Header  |  Block  |  Header |    Free Block    |        
    ///  |          |                                      |                      |          |         |         |                  |
    ///  +-------------------------------------------------+                      +----------+---------+---------+------------------+
    ///                                                                                                |
    ///                                                                                                |
    ///                                                                                                +-----> New block created 
    /// ```
    /// 
    /// The new block that has been created must be added both to the [`FreeList`], since it is not used yet, and 
    /// to the actual [`Region::blocks`], since it is a new block of the current region
    /// 
    /// The payload of the free block is used to store the data we need. See [`FreeList`] for greater detail.
    pub(crate) unsafe fn take_from_block(&mut self, mut block: NonNull<Node<Block>>, requested_size: usize) -> *mut u8 {
        
        unsafe {
            
            // Payload size aligned
            let layout_size = align(requested_size, mem::size_of::<usize>());
            
            // For small memory requests, the requested size is going to be MIN_BLOCK_SIZE anyway.
            let requested = std::cmp::max(layout_size, MIN_BLOCK_SIZE);
            
            // Calculate the offset where next header will start
            let split_offset = align(BLOCK_HEADER_SIZE + requested, mem::size_of::<usize>());

            // Check if we can actualy split
            let total = block.as_ref().data.size + BLOCK_HEADER_SIZE;

            // The remaining space must be enough for a header + `MIN_BLOCK_SIZE`
            if total >= split_offset + BLOCK_HEADER_SIZE + MIN_BLOCK_SIZE {
                let remaining = total - split_offset - BLOCK_HEADER_SIZE;

                // We take the block out of the Free List before modifying it
                self.free_list.remove_free_block(block);
                block.as_mut().data.is_free = false;

                let new_node_addr = NonNull::new_unchecked((block.as_ptr() as *mut u8).add(split_offset));

                // Adjust block size so that it ends just before the new one
                block.as_mut().data.size = split_offset - BLOCK_HEADER_SIZE;

                let mut region = block.as_mut().data.region;
                let new_block = region.as_mut().data.blocks.insert_after(
                    block, 
                    Block {
                        size: remaining,
                        is_free: true,
                        region,
                    }, 
                    new_node_addr.cast()
                );

                let free_payload_addr = new_node_addr.add(BLOCK_HEADER_SIZE);
                self.free_list.insert_free_block(new_block, free_payload_addr);
            } else {
                // There is no space for splitting so we use the whole block
                self.free_list.remove_free_block(block);
                block.as_mut().data.is_free = false;
            }

            // We return a pointer to the payload (just after de header).
            (block.as_ptr() as *mut u8).add(BLOCK_HEADER_SIZE)
        }
    }
}