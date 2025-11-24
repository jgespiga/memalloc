use std::{alloc::Layout, mem, os::raw::{c_int, c_void}, ptr::{self, NonNull}};

use libc::{mmap, munmap, off_t, size_t};

use crate::{freelist::FreeList, list::{Link, List, Node}, region::{REGION_HEADER_SIZE, Region}};

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



struct OldRegion {
    /// Start direction of the Region returned by [`libc::mmap`]
    start: *mut u8,
    /// Size of the region.
    size: usize,
    /// Pointer to next Region
    next: *mut Region,
    /// Pointer to the previous Region
    prev: *mut Region,
    /// First Block in the Region
    first: List<NonNull<Block>>,
}



/// This is the structure of a block. The fields of the block are it's metadata,
/// content is placed after this header.
/// ```text
/// +----------------+        +
/// |      size      |        |
/// +----------------+        |
/// |   is_free (1b) |        | -> Header
/// +----------------+        |
/// |     region     |        |
/// +----------------+        +       
/// |     Content    |
/// |                |
/// +----------------+
/// ```
pub(crate) struct Block {
    /// Size of the block.
    pub size: usize, 
    /// Flag to tell whether the block is free or not.
    pub is_free: bool,
    /// Region which the block belongs to
    pub region: NonNull<Node<Region>>,
}

impl Block {
    unsafe fn free_list_ptr(current: *mut Block) -> *mut FreeList {
        unsafe {
            (current as *mut u8).add(mem::size_of::<Block>()) as *mut FreeList
        }
    }
}



/// The FreeList is a linked list as well
//type FreeListRefactor = List<Block>;

//TODO: Refactor this into proper LinkedList without using raw pointers.
pub struct MmapAllocator {
    /// Linked list of allocator memory [`Region`]
    regions: List<Region>,
    /// Computer's page size (used for aligment). See [`MmapAllocator::align`]
    page_size: usize,
    /// Linked list of free blocks identified by [`Block::is_free`]
    free_list: FreeList,
}

impl MmapAllocator {
    pub unsafe fn new() -> Self {
        // TODO: definitely need to refactor this.
        page_size();
        unsafe {
            Self {regions: List::new(), page_size: PAGE_SIZE, free_list: FreeList::new()}
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
    fn find_block(&self, layout: Layout) -> Link<Node<Block>> {
        if self.free_list.is_empty() {
            // We have no regions created yet.
            return None;
        }

        // This is the size we need, including aligment
        let needed_size = self.align(layout.size(), mem::size_of::<usize>());

        let mut current = self.free_list.items.first();
        while let Some(block) = current {
            unsafe {
                // TODO: I don't know if this nested type is actually crazy or not.
                if block.as_ref().data.as_ref().data.size >= needed_size {
                    return Some(block.as_ref().data);
                }
            
            current = block.as_ref().next;
            }
        }

        // There is no free block we can use
        None
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

            let mut region = self.regions.append(
                Region {
                    size: region_size - REGION_HEADER_SIZE,
                    blocks: List::new(),
                },

                NonNull::new_unchecked(addr).cast(),
            );

            let block_addr = NonNull::new_unchecked(region.as_ptr().offset(1)).cast();

            let block = region.as_mut().data.blocks.append(
                Block {
                    size: region_size - mem::size_of::<Node<Block>>(),
                    is_free: true,
                    region,
                },
                block_addr,
            );

            self.free_list.insert_free_block(block, block_addr);
            
        }
        

        // should we return Result<T> here?
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
    unsafe fn take_from_block(&mut self, mut block: NonNull<Node<Block>>, requested_size: usize) -> *mut u8 {
        let header_size = mem::size_of::<Node<Block>>();

        unsafe {

            // Payload size aligned
            let requested = self.align(requested_size, mem::size_of::<usize>());

            // Calculate what the remaining size would be if we used this block
            let remaining = block.as_ref().data.size.saturating_sub(requested);

            // We take the block out of the Free List
            self.free_list.remove_free_block(block);

            if remaining > header_size + MIN_BLOCK_SIZE {
                // We have to split the block
                let new_node_addr: NonNull<u8> = 
                    NonNull::new_unchecked((block.as_ptr() as *mut u8)
                    .add(header_size + requested));

                let new_block_size = remaining - header_size;

                let region = block.as_mut().data.region.as_mut();

                let new_block = region.data.blocks.insert_after(
                    block,
                    Block {
                        size: new_block_size,
                        is_free: true,
                        region: block.as_mut().data.region,
                    },
                    new_node_addr.cast()
                );

                // We use the free block payload
                let free_block_addr = 
                    NonNull::new_unchecked((block.as_ptr() as *mut u8)
                    .add(header_size));

                self.free_list.insert_free_block(new_block, free_block_addr);
            } else {
                // Splitting is not worth it
                block.as_mut().data.is_free = false;
            }

            // We return a pointer to the payload.
            // This is the address where the user will place content
            (block.as_ptr() as *mut u8).add(header_size)
        }
    }


    #[inline]
    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let mut block = self.find_block(layout);

        if block.is_none() {
            // There is no block aviable, so we need to allocate a new region
            self.allocate_new_region(layout);
            block = self.find_block(layout);
            
            if block.is_none() {
                // There has been an error, what should we do, panic?
                return ptr::null_mut();
            }
        }

        // It doesn't have any sense to call this function unless `block` is not None
        if let Some(block) = block {
            unsafe { self.take_from_block(block, layout.size()) } 
        } else {
            // Error?
            panic!("Todo");
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