use std::{alloc::Layout, mem, os::raw::{c_int, c_void}, ptr};

use libc::{mmap, munmap, off_t, size_t};

/// Virtual memory page siz of the computer. This is usually 4096.
/// This value should be a constant, but we can't do that since we 
/// don't know the value at compile time.
pub(crate) static mut PAGE_SIZE: usize = 0;

/// This is the minimun block size we want to have. If we are
/// goint to split a block, and the remaining size is less than
/// this value, it does not make any sense to split it.
const MIN_BLOCK_SIZE: usize = mem::size_of::<usize>(); 

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


fn align_for_ptr(to_be_aligned: usize) -> usize{
    (to_be_aligned + mem::size_of::<usize>() - 1) & !(mem::size_of::<usize>() - 1)
} 

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

/// This is the structure of a block. The fields of the block are its metadata,
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
/// 1 -> The block is free, so we can use that memory space for whatever
///      we want since the user won't be using it. 
/// 
/// 2 -> We don't want to add extra overhead for the blocks which are not
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
    prev: *mut Block,
    next: *mut Block,
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

    /// It aligns `to_be_aligned` using `alignment`.
    /// 
    /// This method is used to align region sizes to be a multiple of [`MmapAllocator::page_size`]
    /// and pointers in blocks to be a multiple of the computer's pointer size because memory
    /// direcctions have to be aligned.
    fn align(&self, to_be_aligned: usize, alignment: usize) -> usize {
        (to_be_aligned + alignment - 1) & !(alignment - 1)
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

        // What we really need to allocate is the requested size
        // plus the overhead introduced by out allocator's data structures
        let needed = layout.size() + block_overhead;

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
                return;
            }

            let start = addr as *mut u8;

            let mut region = Box::new(Region {
                start,
                size: region_size,
                // We insert the region at the start of the list
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
            // We have created a new region
            self.len += 1;

            self.insert_free_block(new_block);
        }
        

        // should we return Result<T> here?
    }

    unsafe fn take_from_block(&mut self, block: *mut Block, requested_size: usize) -> *mut u8 {
        let header_size = mem::size_of::<Block>();

        unsafe {
            // Calculate what the remaining size would be if we used this block.
            let remaining = (*block).size.saturating_sub(requested_size);

            if remaining > header_size + MIN_BLOCK_SIZE {
                // We have to split the block.

                let new_block_addr = (block as *mut u8).add(self.align(header_size + requested_size, mem::size_of::<usize>())) as *mut Block;

                // New free block
                (*new_block_addr).size = remaining - header_size;
                (*new_block_addr).is_free = true;

                (*new_block_addr).prev = block;
                (*new_block_addr).next = (*block).next;

                if !(*block).next.is_null() {
                    (*(*block).next).prev = new_block_addr;
                }

                (*block).next = new_block_addr;

                (*block).size = requested_size;
            }

            self.remove_free_block(block);

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
    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }

        let header_size = mem::size_of::<Block>();
        unsafe {
            let mut block = (ptr).sub(header_size) as *mut Block;

            (*block).is_free = true;

            // Should refactor this logic into another function?

            // If the previous block is free, we can merge it with this one.
            if !(*block).prev.is_null() && (*(*block).prev).is_free {
                let prev = (*block).prev;

                (*prev).size += header_size + (*block).size;

                (*prev).next = (*block).next;
                if !(*block).next.is_null() {
                    (*(*block).next).prev = prev;
                }

                block = prev;
            }

            // Now we can try to merge it with the next block
            if (*block).next.is_null() && (*(*block).next).is_free {
                let next = (*block).next;

                (*block).size += header_size + (*next).size;

                (*block).next = (*next).next;
                if !(*block).next.is_null() {
                    (*(*next).next).prev = block;
                }
            }

            self.insert_free_block(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_allocation_mmap() {
        unsafe {
            let mut allocator = MmapAllocator::new();
            let layout = Layout::new::<u32>();
            // Allocated space for two unsigned 32 bit integer.
            let block1 = allocator.alloc(layout);
            let block2 = allocator.alloc(layout);

            *block1 = 2;
            assert_eq!(*block1, 2);

            *block2 = 45;
            assert_eq!(*block2, 45);
        }
    }
}