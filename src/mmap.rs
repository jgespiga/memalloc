use std::{alloc::Layout, mem, os::raw::{c_int, c_void}, ptr};

use libc::{mmap, munmap, off_t, size_t};

/// Virtual memory page siz of the computer. This is usually 4096.
/// This value should be a constant, but we can't do that since we 
/// don't know the value at compile time.
pub(crate) static mut PAGE_SIZE: usize = 0;

/// This is the minimun block size we want to have. If we are
/// goint to split a block, and the remaining size is less than
/// this value:
/// - It does not make any sense to split it.
/// - We wouldn't be able to store the [`FreeList`] block metadata
const MIN_BLOCK_SIZE: usize = mem::size_of::<FreeList>(); 

#[inline]
pub(crate) fn page_size() -> usize {
    unsafe {
        if PAGE_SIZE == 0 {
            PAGE_SIZE = libc::sysconf(libc::_SC_PAGE_SIZE) as usize;
        }

        PAGE_SIZE
    }
}


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
/// user allocates memory since we will be wasting a lot of 
/// space. Also, we cannot assume this regions are adjacent.
/// 
/// Therefor, we are going to use the following data structure
/// which consists in a LinkedList of [`Region`] which inside of them
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
///                                     Free List
/// 
///                     Next free block                Next free block
///                +----------------------+  +--------------------------------------+
///                |                      |  |                                      |
/// +--------------|----------------------|--|----+      +--------------------------|-------------------+
/// |        | +---|--+    +-------+    +-|--|-+  |      |        | +-------+    +--|---+    +-------+  |
/// | Region | | Free | -> | Block | -> | Free |  | ---> | Region | | Block | -> | Free | -> | Block |  |
/// |        | +------+    +-------+    +------+  |      |        | +-------+    +------+    +-------+  |
/// +---------------------------------------------+      +----------------------------------------------+
/// 
/// ```


struct Region {
    /// Start direction of the Region returned by [`libc::mmap`]
    start: *mut u8,
    /// Size of the region.
    size: usize,
    /// Pointer to next Region
    next: *mut Region,
    /// Pointer to the previous Region
    prev: *mut Region,
    /// First Block in the Region
    first: *mut Block,
}

/// This is the structure of a block. The fields of the block are it's metadata,
/// content is placed after this header.
/// ```text
/// +----------------+
/// |      size      |
/// +----------------+
/// |   is_free (1b) |
/// +----------------+
/// |      prev      |
/// +----------------+
/// |      next      |
/// +----------------+
/// |                |
/// |     Content    |
/// |                |
/// +----------------+
/// ```
struct Block {
    /// Size of the block.
    size: usize, 
    /// Flag to tell whether the block is free or not.
    is_free: bool,
    /// Pointer to previous block.
    prev: *mut Block,
    /// Pointer to next block.
    next: *mut Block,
    /// Region which the block belongs to
    region: *mut Region,
}

impl Block {
    unsafe fn free_list_ptr(current: *mut Block) -> *mut FreeList {
        unsafe {
            (current as *mut u8).add(mem::size_of::<Block>()) as *mut FreeList
        }
    }
}

/// Linked list to keep track of free [`Block`].
/// 
/// We store the pointers on the [`Block`] content for two main reasons:
/// 
/// - The block is free, so we can use that memory space for whatever
///      we want since the user won't be using it. 
/// 
/// - We don't want to add extra overhead for the blocks which are not
///      free. Each pointer is 8 bytes in size, so we would be having a lot
///      of used blocks with an extra 16 bytes which are there for nothing
///      while the block is being used by the user.
/// 
/// ```text
/// 
///    Free Block                   Next Free Block
/// 
///               +-----------------------------+
///               |                             |
/// +--------+----|---+           +--------+----|---+
/// | Header |    ˅   |           | Header |    ˅   |
/// +--------+--------+           +--------+--------+
/// 
/// ```
/// 
struct FreeList {
    /// Prev block on the Free List
    prev: *mut Block,
    /// Next block on the Free List
    next: *mut Block,
    /// List size
    len: usize,
}

//TODO: Refactor this into proper LinkedList without using raw pointers.
pub struct MmapAllocator {
    /// Linked list of allocator memory [`Region`]
    regions: *mut Region,
    /// Computer's page size (used for aligment). See [`MmapAllocator::align`]
    page_size: usize,
    /// Number of regions
    len: usize,
    /// Linked list of free blocks identified by [`Block::is_free`]
    free_list: *mut Block,
}

impl MmapAllocator {
    pub unsafe fn new() -> Self {
        // TODO: definitely need to refactor this.
        page_size();
        unsafe {
            Self {regions: ptr::null_mut(), page_size: PAGE_SIZE, len: 0, free_list: ptr::null_mut()}
        }
    }

    /// It aligns `to_be_aligned` using `aligment`.
    /// 
    /// This method is used to align region sizes to be a multiple of [`MmapAllocator::page_size`]
    /// and pointers in blocks to be a multiple of the computer's pointer size because memory
    /// direcctions have to be aligned.
    fn align(&self, to_be_aligned: usize, aligment: usize) -> usize {
        (to_be_aligned + aligment - 1) & !(aligment - 1)
    }


    /// Returns a pointer to the [`Block`] where we can allocate `layout`.
    /// This is done by iterating through the [`FreeList`] and searching for
    /// a block that can allocate enough `size`
    fn find_block(&self, layout: Layout) -> *mut Block {
        if self.free_list.is_null() {
            // We have no regions created yet.
            return ptr::null_mut();
        }

        // This is the size we need, including aligment
        let needed_size = self.align(layout.size(), mem::size_of::<usize>());

        let mut current = self.free_list;
        while !current.is_null() {
            unsafe {
                if (*current).size >= needed_size {
                    // We have found a block which has enough size
                    return current;
                }
                
                let links = Block::free_list_ptr(current);
                current = (*links).next;
            }
        }

        // There is no free block we can use
        ptr::null_mut()
    }

    /// This function calls to mmap, and returns a new memory region that can
    /// handle a given size.
    /// 
    /// If [`MmapAllocator::find_block`] returns null pointer, we know for
    /// sure there is no way we can allocate the requested size on our current
    /// Regions. Therefor, we need to allocate a new [`Region`] using
    /// [`libc::mmap`].
    /// 
    /// This implementation is platform-dependant. It only works on linux right now.
    fn allocate_new_region(&mut self, layout: Layout) -> () {
        let block_overhead = mem::size_of::<Block>();

        // What we really need to allocate is the requested size (aligned)
        // plus the overhead introduced by out allocator's data structures
        let needed_payload = self.align(layout.size(), mem::size_of::<usize>());
        let needed = needed_payload + block_overhead;

        let region_size = self.align(needed, self.page_size);

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
                panic!("mmap failed trying to asign {} bytes", region_size);
            }

            let start = addr as *mut u8;

            let mut region = Box::new(Region {
                start,
                size: region_size,
                // We insert the region at the start of the list
                next: self.regions,
                prev: ptr::null_mut(),
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

            let region_ptr = Box::into_raw(region);

            if !self.regions.is_null() {
                (*self.regions).prev = region_ptr;
            }

            (*new_block).region = region_ptr;
            self.regions = region_ptr;

            // We have created a new region
            self.len += 1;
            self.insert_free_block(new_block);
            
        }
        

        // should we return Result<T> here?
    }

    unsafe fn take_from_block(&mut self, block: *mut Block, requested_size: usize) -> *mut u8 {
        let header_size = mem::size_of::<Block>();

        unsafe {

            // Payload size aligned
            let requested = self.align(requested_size, mem::size_of::<usize>());

            // Calculate what the remaining size would be if we used this block
            let remaining = (*block).size.saturating_sub(requested);

            // We take the block out of the Free List.
            self.remove_free_block(block);

            if remaining > header_size + MIN_BLOCK_SIZE {
                // We have to split the block
                let new_free_block = (block as *mut u8)
                    .add(header_size + requested) as *mut Block;

                // New free block
                (*new_free_block).size = remaining - header_size;
                (*new_free_block).is_free = true;
                (*new_free_block).prev = block;
                (*new_free_block).next = (*block).next;
                (*new_free_block).region = (*block).region;

                if !(*block).next.is_null() {
                    (*(*block).next).prev = new_free_block;
                }
                
                (*block).next = new_free_block;
                (*block).size = requested;

                // We can insert `new_free_block` on the FreeList
                self.insert_free_block(new_free_block);
            }

            // We return a pointer to the payload.
            // This is the address where the user will place content
            (block as *mut u8).add(header_size)
        }
    }


    #[inline]
    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let mut block = self.find_block(layout);

        if block.is_null() {
            // There is no block aviable, so we need to allocate a new region
            self.allocate_new_region(layout);
            block = self.find_block(layout);
            
            if block.is_null() {
                // There has been an error, what should we do, panic?
                return ptr::null_mut();
            }
        }
        unsafe { self.take_from_block(block, layout.size()) }
    }



    unsafe fn insert_free_block(&mut self, block: *mut Block) {
        
        unsafe {
            (*block).is_free = true;

            let links = Block::free_list_ptr(block);

            // We insert free block at the start of the list to avoid iterating through it.
            (*links).prev = std::ptr::null_mut();
            (*links).next = self.free_list;

            if !self.free_list.is_null() {
                let head_links = Block::free_list_ptr(self.free_list);
                (*head_links).prev = block;
            }
            
            self.free_list = block;
        }
    }

    unsafe fn remove_free_block(&mut self, block: *mut Block) {
        unsafe {
            let links = Block::free_list_ptr(block);
            let prev = (*links).prev;
            let next = (*links).next;

            if !prev.is_null() {
                let prev_links = Block::free_list_ptr(prev);
                (*prev_links).next = next;
            } else {
                self.free_list = next;
            }

            if !next.is_null() {
                let next_links = Block::free_list_ptr(next);
                (*next_links).prev = prev;
            }
            
            (*links).prev = ptr::null_mut();
            (*links).next = ptr::null_mut();
            (*block).is_free = false;
        }
    }

    #[inline]
    pub unsafe fn dealloc(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }

        let header_size = mem::size_of::<Block>();
        unsafe {
            let mut block = (ptr).sub(header_size) as *mut Block;

            // If it is already free, we don't do anything
            if (*block).is_free {
                return;
            }

            (*block).is_free = true;

            let region = (*block).region;

            // Should refactor this logic into another function?

            // If the previous block is free, we can merge it with this one.
            if !(*block).prev.is_null() && (*(*block).prev).is_free {
                let prev = (*block).prev;
                
                if (*prev).size >= MIN_BLOCK_SIZE {
                    self.remove_free_block(prev);
                }

                // TODO: I don't know what is this                
                (*prev).size += header_size + (*block).size;
                (*prev).next = (*block).next;

                if !(*block).next.is_null() {
                    (*(*block).next).prev = prev;
                }

                block = prev;
            }

            // Now we can try to merge it with the next block
            if !(*block).next.is_null() && (*(*block).next).is_free {
                let next = (*block).next;

                if (*next).size >= MIN_BLOCK_SIZE {
                    self.remove_free_block(next);
                }

                (*block).size += header_size + (*next).size;
                (*block).next = (*next).next;

                if !(*block).next.is_null() {
                    (*(*block).next).prev = block;
                }
            }
            

            // Now we have to check if the region has only one free block.
            // In that case, we need to delete the region from the Linked List
            // and call `munmap` on it.

            if (*block).prev.is_null() && (*block).next.is_null() {
                // The current Region is free, so we can munmap it

                let prev_region = (*region).prev;
                let next_region = (*region).next;

                if !prev_region.is_null() {
                    (*prev_region).next = next_region;
                } else {
                    self.regions = next_region;
                }

                if !next_region.is_null() {
                    (*next_region).prev = prev_region;
                }

                // We have deleted the Region
                self.len -= 1;

                munmap((*region).start as *mut c_void, (*region).size as size_t);

                // Free the Region struct (TODO: I'm not sure about this)
                let _ = Box::from_raw(region);
            } else {
                // If the current Region still in use:
                // Once we have finished merging all the posible blocks, we
                // can insert the entire block on the FreeList
                self.insert_free_block(block);  
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_pointer_size() {
        let aligments = vec![(1..8, 8), (9..16, 16), (17..24, 24), (25..32, 32)];
        unsafe { 
            let allocator = MmapAllocator::new();

            for (sizes, expected) in aligments {
                for size in sizes {
                    assert_eq!(expected, allocator.align(size, mem::size_of::<usize>()));
                }
            }
        }
    }

    #[test]
    fn align_page_size() {
        let aligments = vec![(1..4096, 4096), (4097..8192, 8192)];

        unsafe {
            let allocator = MmapAllocator::new();

            for (sizes, expected) in aligments {
                for size in sizes {
                    assert_eq!(expected, allocator.align(size, allocator.page_size))
                }
            }
        }

    }

    #[test]
    fn basic_allocation_and_write() {
        unsafe {
            let mut allocator = MmapAllocator::new();
            let layout = Layout::new::<u32>();

            let block1 = allocator.alloc(layout) as *mut u32;

            *block1 = 12415;
            assert_eq!(*block1, 12415);

            let block2 = allocator.alloc(layout) as *mut u32;

            *block2 = 36353;
            assert_eq!(*block2, 36353);

            // Check block1 has not been overwritten
            assert_eq!(*block1, 12415);
        }
    }

    #[test]
    fn alloc_dealloc_reuse() {
        unsafe {
            let mut allocator = MmapAllocator::new();
            let layout = Layout::new::<u64>();

            let block1 = allocator.alloc(layout);
            assert!(!block1.is_null());

            // We free the block
            allocator.dealloc(block1);

            let block2 = allocator.alloc(layout);
            assert!(!block2.is_null());

            assert_eq!(block1, block2);

            let block3 = allocator.alloc(layout);
            assert!(!block3.is_null());

            // Whe should get a different block since we haven't deallocated `block2`
            assert_ne!(block3, block2);            
        }
    }

    #[test]
    fn dealloc_null() {
        unsafe {
            // This should not do anything, it should not panic.
            let mut allocator = MmapAllocator::new();
            allocator.dealloc(ptr::null_mut());
        }
    }

    // TODO: Fix this
    // #[test]
    // fn double_free() {
    //     unsafe {
    //         let mut allocator = MmapAllocator::new();
    //         let layout = Layout::new::<u32>();

    //         let block1 = allocator.alloc(layout);
            
    //         allocator.dealloc(block1);

    //         // This should not do anything since the block is already free.
    //         allocator.dealloc(block1);

    //         // Check everything continues working 
    //         let block2 = allocator.alloc(layout) as *mut u32;
    //         assert!(!block2.is_null());

    //         *block2 = 124;
    //         assert_eq!(*block2, 124);
    //     }
    // }


    #[test]
    fn block_merging() {
        unsafe {
            let mut allocator = MmapAllocator::new();

            // Space for a block of 128 bytes
            let layout = Layout::new::<[u8; 128]>();

            let p1 = allocator.alloc(layout);
            let p2 = allocator.alloc(layout);
            let p3 = allocator.alloc(layout);
            let p4 = allocator.alloc(layout);

            assert!(!p1.is_null() && !p2.is_null() && !p3.is_null() && !p4.is_null());

            allocator.dealloc(p1);
            allocator.dealloc(p3);

            // This should be merged with `prev` (p1) and `next` (p2)
            allocator.dealloc(p2);

            let p5 = allocator.alloc(Layout::new::<[u8; 264]>());
            assert!(!p5.is_null());

            // If blocks have been merged, the big layout we have just allocated `p4`
            // should return `p1` as a bigger block.
            assert_eq!(p1, p5);
        }
    }

    #[test]
    fn munmap_region_when_needed() {
        unsafe {
            let mut allocator = MmapAllocator::new();
            let layout = Layout::new::<u64>();

            let p1 = allocator.alloc(layout);
            let p2 = allocator.alloc(layout);

            assert!(!allocator.regions.is_null());

            allocator.dealloc(p1);
            allocator.dealloc(p2);

            assert!(allocator.regions.is_null());
        }
    }
}