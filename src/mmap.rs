use std::{alloc::Layout, mem, ptr::{self, NonNull}};

use crate::{
    block::{BLOCK_HEADER_SIZE, Block}, 
    kernel::{self, Kernel}, 
    list::{Link, List, Node}, 
    region::{REGION_HEADER_SIZE, Region}, 
    utils::align,
};


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






/// The main allocator's Struct. 
/// 
/// This is a wrapper over [`Kernel`], see that for more detail of the internals
/// of the allocator.
pub struct MmapAllocator {
    allocator: Kernel,
}

impl MmapAllocator {
    /// Construct a new allocator by constructing its `Kernel`
    pub unsafe fn new() -> Self {
        Self { allocator: Kernel::new() }
    }


    

    #[inline]
    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let mut block = self.allocator.free_list.find_free_block(layout);

        if block.is_none() {
            // There is no block aviable, so we need to allocate a new region
            self.allocator.allocate_new_region(layout).unwrap();
            block = self.allocator.free_list.find_free_block(layout);
            
            if block.is_none() {
                // There has been an error, what should we do, panic?
                return ptr::null_mut();
            }
        }

        // It doesn't have any sense to call this function unless `block` is not None
        if let Some(block) = block {
            unsafe { self.allocator.take_from_block(block, layout.size()) } 
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
            self.allocator.check_region_removal(&mut region, block_node);
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