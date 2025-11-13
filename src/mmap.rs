use std::{alloc::Layout, os::raw::{c_int, c_void}, ptr};

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


struct MmapAllocator {
    /// Linked list of allocator memory [`Region`]
    regions: *mut Region,
    /// Computer's page size (used for aligment?)
    page_size: usize,
}

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