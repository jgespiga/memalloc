use std::{alloc::Layout, mem, ptr::{self, NonNull}};

use crate::{block::{BLOCK_HEADER_SIZE, Block}, kernel::{self, Kernel}, list::{Link, List, Node}, region::{REGION_HEADER_SIZE, Region}, utils::align};


/// This is the minimun block size we want to have. If we are
/// goint to split a block, and the remaining size is less than
/// this value:
/// - It does not make any sense to split it.
/// - We wouldn't be able to store the [`FreeList`] block metadata
pub(crate) const MIN_BLOCK_SIZE: usize = mem::size_of::<Node<NonNull<Node<Block>>>>(); 



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






/// Allocator structure.
pub struct MmapAllocator {
    allocator: Kernel,
}

impl MmapAllocator {
    /// Construct a new allocator by constructing its `Kernel`
    pub unsafe fn new() -> Self {
        Self { allocator: Kernel::new() }
    }


    /// Returns a pointer to the [`Block`] where we can allocate `layout`.
    /// This is done by iterating through the [`FreeList`] and searching for
    /// a block that can allocate enough `size`.
    /// 
    /// This implementation of the method uses the first-fit algorithm, it returns
    /// the first block on the [`FreeList`] that we can use.
    fn find_free_block(&self, layout: Layout) -> Link<Node<Block>> {
        if self.allocator.free_list.is_empty() {
            // We have no regions created yet.
            return None;
        }

        // This is the size we need, including aligment
        let layout_size = align(layout.size(), mem::size_of::<usize>());

        // The minimun block size we can give to the user is `MIN_BLOCK_SIZE`. If we
        // didn't do this, we wouldn't be able to store our allocator's metadata on
        // small memory requests.
        let needed_size = std::cmp::max(layout_size, MIN_BLOCK_SIZE);
        

        // We check in our free_list if there exists any node that can fit `needed_size`
        for node in &self.allocator.free_list.items {
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
    fn allocate_new_region(&mut self, layout: Layout) -> Result<(), &'static str> {
        let block_overhead = mem::size_of::<Node<Block>>();

        // What we really need to allocate is the requested size (aligned)
        // plus the overhead introduced by out allocator's data structures
        let layout_size = align(layout.size(), mem::size_of::<usize>());

        // The minimun block size we can give to the user is `MIN_BLOCK_SIZE`. If we
        // didn't do this, we wouldn't be able to store our allocator's metadata on
        // small memory requests.
        let needed_payload = std::cmp::max(layout_size, MIN_BLOCK_SIZE);

        let needed = needed_payload + block_overhead;

        let region_size = align(needed, self.allocator.page_size);

        unsafe {    
            // What should we do here? I assume its okay to panic if 
            // we get None from calling `mmap`.
            let addr = kernel::request_memory(region_size).expect("mmap syscall returned None");

            let mut region = self.allocator.regions.append(
                Region {
                    size: region_size - REGION_HEADER_SIZE,
                    blocks: List::new(),
                },

                addr
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

            self.allocator.free_list.insert_free_block(block, free_node_addr);
        }
        
        Ok(())
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

        unsafe {

            // Payload size aligned
            let layout_size = align(requested_size, mem::size_of::<usize>());

            // For small memory requests, the requested size is going to be MIN_BLOCK_SIZE anyway.
            let requested = std::cmp::max(layout_size, MIN_BLOCK_SIZE);

            // Calculate what the remaining size would be if we used this block
            let remaining = block.as_ref().data.size.saturating_sub(requested);

            // We take the block out of the Free List
            self.allocator.free_list.remove_free_block(block);

            // We are going to use this block, so we marked as used
            block.as_mut().data.is_free = false;

            if remaining > BLOCK_HEADER_SIZE + MIN_BLOCK_SIZE {
                // We have to split the block
                let new_node_addr: NonNull<u8> = 
                    NonNull::new_unchecked((block.as_ptr() as *mut u8)
                    .add(BLOCK_HEADER_SIZE + requested));

                let new_block_size = remaining - BLOCK_HEADER_SIZE;

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
                    .add(BLOCK_HEADER_SIZE));

                self.allocator.free_list.insert_free_block(new_block, free_block_addr);
            } else {
                // Splitting is not worth it
                block.as_mut().data.is_free = false;
            }

            // We return a pointer to the payload.
            // This is the address where the user will place content
            (block.as_ptr() as *mut u8).add(BLOCK_HEADER_SIZE)
        }
    }


    #[inline]
    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let mut block = self.find_free_block(layout);

        if block.is_none() {
            // There is no block aviable, so we need to allocate a new region
            self.allocate_new_region(layout).unwrap();
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

            // Try to merge the block with the previous one.
            region.as_mut().data.merge_with_prev(&mut block_node);

            // Try to merge the block with the next one.
            region.as_mut().data.merge_with_next(&mut block_node, &mut self.allocator.free_list);

            // Check if we need to remove and munmap the current `region`
            self.check_region_removal(&mut region, block_node);
        }
    }
    
    /// Checks if the given `region` needs to be returned to the OS or not while managing
    /// the free_list and the state of `block` which might be the only block left in the region.
    /// We need this `block` since, if we were to munmap this region, we also need to remove that block
    /// from out free_list to avoid future problems. If we didn't do that, our allocator could think that
    /// this `block` stills free and therefor it will try to use it, causing undefined behavior.
    fn check_region_removal(&mut self, region: &mut NonNull<Node<Region>>, block: NonNull<Node<Block>>) {
        unsafe {
            if region.as_mut().data.blocks.len() == 1 {
                let total_region_size = region.as_ref().data.size + REGION_HEADER_SIZE;
                
                // Just in case the block stills in the free list, we always remove it.
                // If it was not in the free list, `remove_free_block` will manage it
                self.allocator.free_list.remove_free_block(block);
                self.allocator.regions.remove(*region);
                
                let region_start = region.as_ptr() as *mut u8;

                kernel::return_memory(region_start, total_region_size);
            } else {
                // The current region still has other blocks so the merged block has to return to the free list.
                
                // We remove the block from the list, and we reinsert it with the correct size.
                self.allocator.free_list.remove_free_block(block);
                // We use the free block payload
                let free_block_addr = 
                NonNull::new_unchecked((block.as_ptr() as *mut u8)
                .add(BLOCK_HEADER_SIZE));
            
                self.allocator.free_list.insert_free_block(block, free_block_addr);
            }
        }
    }
      
}

#[cfg(test)]
mod tests {
    use super::*;

    

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

            assert!(!allocator.allocator.regions.is_empty());
            
            allocator.dealloc(p1);
            allocator.dealloc(p2);

            assert!(allocator.allocator.regions.is_empty());
        }
    }
}