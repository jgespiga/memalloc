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
const MIN_BLOCK_SIZE: usize = mem::size_of::<Node<NonNull<Node<Block>>>>(); 

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
    /// a block that can allocate enough `size`.
    /// 
    /// This implementation of the method uses the first-fit algorithm, it returns
    /// the first block on the [`FreeList`] that we can use.
    fn find_free_block(&self, layout: Layout) -> Link<Node<Block>> {
        if self.free_list.is_empty() {
            // We have no regions created yet.
            return None;
        }

        // This is the size we need, including aligment
        let layout_size = self.align(layout.size(), mem::size_of::<usize>());

        // The minimun block size we can give to the user is `MIN_BLOCK_SIZE`. If we
        // didn't do this, we wouldn't be able to store our allocator's metadata on
        // small memory requests.
        let needed_size = std::cmp::max(layout_size, MIN_BLOCK_SIZE);
        
        //TODO: for node in &self.free_list.items {}

        // We check in our free_list if there exists any node that can fit `needed_size`
        for node in &self.free_list.items {
            unsafe {
                if node.as_ref().data.size >= needed_size {
                    // We found a node that we can use
                    return Some(*node);
                }
            }
        }


        // There is no free block we can use
        None
    }

    /// This function calls to mmap, and returns a new memory region that can
    /// handle a given size.
    /// 
    /// If [`MmapAllocator::find_free_block`] returns null pointer, we know for
    /// sure there is no way we can allocate the requested size on our current
    /// Regions. Therefor, we need to allocate a new [`Region`] using
    /// [`libc::mmap`].
    /// 
    /// This implementation is platform-dependant. It only works on linux right now.
    fn allocate_new_region(&mut self, layout: Layout) -> () {
        let block_overhead = mem::size_of::<Node<Block>>();

        // What we really need to allocate is the requested size (aligned)
        // plus the overhead introduced by out allocator's data structures
        let layout_size = self.align(layout.size(), mem::size_of::<usize>());

        // The minimun block size we can give to the user is `MIN_BLOCK_SIZE`. If we
        // didn't do this, we wouldn't be able to store our allocator's metadata on
        // small memory requests.
        let needed_payload = std::cmp::max(layout_size, MIN_BLOCK_SIZE);

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

            // First Node<Block> right after Node<Region>
            let block_addr = NonNull::new_unchecked(region.as_ptr().offset(1)).cast();

            // Useful block size
            let block_size = region.as_ref().data.size - mem::size_of::<Node<Block>>();

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
                block.as_ptr().cast::<u8>().add(mem::size_of::<Node<Block>>())
            );

            self.free_list.insert_free_block(block, free_node_addr);
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
            let layout_size = self.align(requested_size, mem::size_of::<usize>());

            // For small memory requests, the requested size is going to be MIN_BLOCK_SIZE anyway.
            let requested = std::cmp::max(layout_size, MIN_BLOCK_SIZE);

            // Calculate what the remaining size would be if we used this block
            let remaining = block.as_ref().data.size.saturating_sub(requested);

            // We take the block out of the Free List
            self.free_list.remove_free_block(block);

            // We are going to use this block, so we marked as used
            block.as_mut().data.is_free = false;

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
                    NonNull::new_unchecked((new_block.as_ptr() as *mut u8)
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
        let mut block = self.find_free_block(layout);

        if block.is_none() {
            // There is no block aviable, so we need to allocate a new region
            self.allocate_new_region(layout);
            block = self.find_free_block(layout);
            
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

        let header_size = mem::size_of::<Node<Block>>();
        
        unsafe {

            // We assume this part is a `header`, if is not, this will be UB
            let block_node_ptr = ptr.sub(header_size) as *mut Node<Block>;
            let mut block_node = NonNull::new_unchecked(block_node_ptr);

            // Block data
            let block = &mut block_node.as_mut().data;

            // If it is already free, we don't do anything
            if block.is_free {
                return;
            }

            block.is_free = true;

            let mut region = block.region;

            // Block merging (TODO: extract this into other functions)

            // If the previous block is free, we can merge it with this one.
            if let Some(mut prev_node) = block_node.as_ref().prev {

                let prev_block = &mut prev_node.as_mut().data;

                // We remove the prev node from the free list since we are going to merge it.
                if prev_block.is_free {
                    // As prev_block is already in the `free list` we just need to increment its size
                    // and remove its adjacent block with which we are going to merge this one from the list
                    
                    // We need to cover the header and the actual content of the block
                    prev_block.size += header_size + block.size;
                    
                    // We remove the block from the list since it is going to be merged
                    region.as_mut().data.blocks.remove(block_node);

                    // The current block is now its previous one
                    block_node = prev_node;
                }
            }

            // Now, `block_node` is the block merged with the previous one (if there was)
            // Therefor, we can try to merge it with the next block
            if let Some(mut next_node) = block_node.as_ref().next {
                let next_block = &mut next_node.as_mut().data;

                if next_block.is_free {
                    if next_block.size >= MIN_BLOCK_SIZE {
                        self.free_list.remove_free_block(next_node);
                    }

                    block_node.as_mut().data.size += header_size + next_block.size;
                    // We remove the block from the list since it is going to be merged                   
                    region.as_mut().data.blocks.remove(next_node);
               }
            }


            // Now we have to check if the region has only one free block.
            // In that case, we need to delete the region from the Linked List and call `munmap` on it.
            if block_node.as_ref().prev.is_none() && block_node.as_ref().next.is_none() {
                println!("Borrando region");
                let total_region_size = region.as_ref().data.size + REGION_HEADER_SIZE;

                // Just in case the block stills in the free list, we always remove it just in case.
                // If it was not in the free list, `remove_free_block` will manage it
                self.free_list.remove_free_block(block_node);
                self.regions.remove(region);

                let region_start = region.as_ptr() as *mut u8;

                munmap(region_start as *mut c_void, total_region_size as size_t);
            } else {
                // The current region still has other blocks so the merged block has to return to the free list.

                // We remove the block from the list, and we reinsert it with the correct size.
                // TODO: Performance?
                self.free_list.remove_free_block(block_node);
                // We use the free block payload
                let free_block_addr = 
                    NonNull::new_unchecked((block_node.as_ptr() as *mut u8)
                    .add(header_size));

                self.free_list.insert_free_block(block_node, free_block_addr);
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

            // Avoid munmaping the region during the test
            allocator.alloc(Layout::new::<u64>());

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

    #[test]
    fn block_merging() {
        unsafe {
            let mut allocator = MmapAllocator::new();
            let layout = Layout::new::<u8>();

            // Avoid munmaping the region during the test
            allocator.alloc(Layout::new::<u64>());

            let p1 = allocator.alloc(layout);
            let p2 = allocator.alloc(layout);
            allocator.dealloc(p2);

            // After this, p1 and p2 should be merged (test: merging with next)            
            allocator.dealloc(p1);
            // This block should use the previosly merged block since p1 + p2 = 16
            let p3 = allocator.alloc(Layout::new::<u16>());
            assert_eq!(p1, p3);

            let layout2 = Layout::new::<u16>();
            let p4 = allocator.alloc(layout2);

            allocator.dealloc(p3);

            //After this, p3 and p4 should be merged (test: merging with prev)
            allocator.dealloc(p4);            

            // This block should use the previosly merged block since p3 + p4 = 32
            let p5 = allocator.alloc(Layout::new::<u32>());
            assert_eq!(p3, p5);

        }
    }

    #[test]
    fn munmap_region_when_needed() {
        unsafe {
            let mut allocator = MmapAllocator::new();
            let layout = Layout::new::<u64>();

            let p1 = allocator.alloc(layout);
            let p2 = allocator.alloc(layout);

            assert!(!allocator.regions.is_empty());

            allocator.dealloc(p1);
            allocator.dealloc(p2);

            assert!(allocator.regions.is_empty());
        }
    }
}