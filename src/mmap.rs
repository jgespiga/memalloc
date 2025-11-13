use std::{alloc::Layout, mem, os::raw::{c_int, c_void}, ptr};

use libc::{mmap, munmap, off_t, size_t};


/// Virtual memory layout of a process
/// ```text
/// +-------------------------+
/// |   Kernel virtual memory |  | -> invisible to the user code
/// +-------------------------+
/// |                         |
/// |          Stack          |
/// |                         |
/// +-------------------------+
/// |                         |
/// |                         |
/// |                         |
/// |                         |
/// +-------------------------+
/// |                         |
/// |          Heap           |
/// |                         |
/// +-------------------------+
/// 
/// ... Read/write and Read-only segments
/// 
/// ```



/// [`libc::mmap`] gives as memory regions aligned with the computer
/// page size. But, we cannot use a full Region each time
/// user allocates memory since we will we wasting a lot of 
/// space. Also, we cannot assume this regions are adjacent.
/// 
/// Therefor, we are going to use the following data structure
/// which consists in a LinkedList of Regions which inside of them
/// have a LinkedList of [`Block`].
/// 
/// ```text
/// +-----------------------------------------------+      +-----------------------------------------------+
/// |        | +-------+    +-------+    +-------+  |      |        | +-------+    +-------+    +-------+  |
/// | Region | | Block | -> | Block | -> | Block |  | ---> | Region | | Block | -> | Block | -> | Block |  |
/// |        | +-------+    +-------+    +-------+  |      |        | +-------+    +-------+    +-------+  |
/// +-----------------------------------------------+      +-----------------------------------------------+
/// ```
/// 
/// We also need to keep track of the free blocks. So now we store
/// two separate LinkedLists. One for memory [`Region`] and the other
/// one of free [`Block`] which you can identify by the [`Block::is_free`]
/// flag.
/// 
/// So our [`MmapAllocator`] looks something like this:
/// 
/// ```text
///                                           Free List
/// 
///                     Next free block                Next free block
///                +---------------------+  +--------------------------------------+
///                |                     |  |                                      |
/// +--------------|---------------------|--|----+      +--------------------------|-------------------+
/// |        | +---|--+    +------+    +-|--|-+  |      |        | +-------+    +--|---+    +-------+  |
/// | Region | | Free | -> | Block | ->| Free |  | ---> | Region | | Block | -> | Free | -> | Block |  |
/// |        | +------+    +------+    +------+  |      |        | +-------+    +------+    +-------+  |
/// +--------------------------------------------+      +----------------------------------------------+
/// 
/// ```


struct Region {
    /// Start direction of the Region returned by [`libc::mmap`]
    start: *mut u8,
    /// Size of the region.
    size: usize,
    /// Pointer to next Region
    next: *mut Region,
    /// First Block in the Region
    first: *mut Block,
}

struct Block {
    /// Size of the block.
    size: usize, 
    /// Flag to tell whether the block is free or not.
    is_free: bool,
    /// Pointer to previous block.
    prev: *mut Block,
    /// Pointer to next block.
    next: *mut Block,
}


//TODO: Refactor this into proper LinkedList without using raw pointers.
struct MmapAllocator {
    /// Linked list of allocator memory [`Region`]
    regions: *mut Region,
    /// Computer's page size (used for aligment). See [`MmapAllocator::align`]
    page_size: usize,
    /// Number of regions
    len: usize,
}

impl MmapAllocator {

    /// It aligns `to_be_aligned` to be a multiple of [`MmapAllocator::page_size`].
    fn align(&self, to_be_aligned: usize) -> usize {
        (to_be_aligned + self.page_size - 1) & !(self.page_size - 1)
    }

    /// Finds a block in a given [`Region`] that can allocate
    /// the requested size.
    fn find_block_in_region(&self, layout: Layout, region: &Region) -> *mut Block {
        // We assume first can't be null on a region since, if this was
        // the case, the region should have been unmapped and returned to
        // the kernel.
        let mut current = region.first;

        unsafe {
            while !current.is_null() {
                if !(*current).is_free {
                    current = (*current).next;
                    continue;
                }
                
                if (*current).size >= layout.size() {
                    return current;
                }
                current = (*current).next;
            }
        }

        ptr::null_mut()
    }

    /// Returns a pointer to the [`Block`] where we can allocate `layout`.
    /// If no Region with enough size is found, we return null pointer.
    fn find_block(&self, layout: Layout) -> *mut Block {
        if self.len == 0 {
            // We have no regions created yet.
            return ptr::null_mut();
        }

        let mut current = self.regions;
        while !current.is_null() {
            unsafe {
                if (*current).size >= layout.size() {
                    // We search for a block at current region.
                    let block = self.find_block_in_region(layout, &(*current));

                    if !block.is_null() {
                        return block;
                    }
                }
                current = (*current).next;
            }
        }

        ptr::null_mut()
    }


    /// If [`MmapAllocator::find_block`] returns null pointer, we know for
    /// sure there is no way we can allocate the requested size on our current
    /// Regions. Therefor, we need to allocate a new [`Region`] using
    /// [`libc::mmap`].
    
    /// This function calls to mmap, and returns a new memory region that can
    /// handle a given size.
    fn allocate_new_region(&mut self, layout: Layout) -> () {
        let block_overhead = mem::size_of::<Block>();

        // What we really need to allocate is the requested size
        // plus the overhead introduced by out allocator's data structures
        let needed = layout.size() + block_overhead;

        let region_size = self.align(needed);

        const ADDR: *mut c_void = ptr::null_mut::<c_void>();
        const PROT: c_int = libc::PROT_READ | libc::PROT_WRITE;
        const FLAGS: c_int = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
        const FD: c_int = -1;
        const OFFSET: off_t = 0;

        unsafe {    
            let addr = mmap(ADDR, region_size as size_t, PROT, FLAGS, FD, OFFSET);
            
            if addr == libc::MAP_FAILED {
                // TODO: We should refactor this to return Result<T> so we
                // can propagate the error
                return;
            }

            let start = addr as *mut u8;

            let mut region = Box::new(Region {
                start,
                size: region_size,
                // We insert the region at the start of the list.
                next: self.regions,
                first: ptr::null_mut(),
            });

            // The first block is going to be marked as free to use and
            // it fills the whole region size.
            let new_block = start as *mut Block;
            (*new_block).size = region_size - block_overhead;
            (*new_block).is_free = true;
            (*new_block).prev = ptr::null_mut();
            (*new_block).next = ptr::null_mut();

            region.first = new_block;

            self.regions = Box::into_raw(region);
        }

        // should we return Result<T> here?
    }


    #[inline]
    pub const fn new() -> Self {
        Self {regions: ptr::null_mut(), page_size: 4096, len: 0}
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