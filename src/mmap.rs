use std::{alloc::{GlobalAlloc, Layout}, mem, ptr::{self, NonNull}, sync::Mutex};

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
/// of the allocator. The kernel is behind a `Mutex` in order to allow secure mutability.
pub struct MmapAllocator {
    allocator: Mutex<Kernel>,
}

impl MmapAllocator {
    /// Construct a new allocator by constructing its `Kernel`
    pub unsafe fn new() -> Self {
        Self { allocator: Mutex::new(Kernel::new()) }
    }


    #[inline]
    pub unsafe fn allocate(&self, layout: Layout) -> *mut u8 {

        // We adquire the lock. If any other user panics while having the lock
        // of this mutex, we should panic the program too, because that would mean
        // that our allocator has had an unrecovable error. Therefor, we can unwrap 
        // this Result.
        let mut kernel = self.allocator.lock().unwrap();

        let mut block = kernel.free_list.find_free_block(layout);

        if block.is_none() {
            // There is no block aviable, so we need to allocate a new region
            kernel.allocate_new_region(layout).unwrap();
            block = kernel.free_list.find_free_block(layout);
            
            if block.is_none() {
                // There has been an error, what should we do, panic?
                return ptr::null_mut();
            }
        }

        // It doesn't have any sense to call this function unless `block` is not None
        if let Some(block) = block {
            unsafe { kernel.take_from_block(block, layout.size()) } 
        } else {
            // Error?
            panic!("Todo");
        }
    }
    

    #[inline]
    pub unsafe fn deallocate(&self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }

        // We lock the mutex
        let mut kernel = self.allocator.lock().unwrap();

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
            region.as_mut().data.merge_with_next(&mut block_node, &mut kernel.free_list);

            // Check if we need to remove and munmap the current `region`
            kernel.check_region_removal(&mut region, block_node);
        }
    }
}

unsafe impl GlobalAlloc for MmapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.allocate(layout) }
    }
    
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_allocation_and_write() {
        unsafe {
            let allocator = MmapAllocator::new();
            let layout = Layout::new::<u32>();
            
            let block1 = allocator.allocate(layout) as *mut u32;

            *block1 = 12415;
            assert_eq!(*block1, 12415);

            let block2 = allocator.allocate(layout) as *mut u32;

            *block2 = 36353;
            assert_eq!(*block2, 36353);

            // Check block1 has not been overwritten
            assert_eq!(*block1, 12415);
        }
    }

    #[test]
    fn alloc_dealloc_reuse() {
        unsafe {
            let allocator = MmapAllocator::new();
            let layout = Layout::new::<u64>();

            // Avoid munmaping the region during the test
            allocator.allocate(Layout::new::<u64>());

            let block1 = allocator.alloc(layout);
            assert!(!block1.is_null());

            // We free the block
            allocator.deallocate(block1);

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
            let allocator = MmapAllocator::new();
            allocator.deallocate(ptr::null_mut());
        }
    }

    #[test]
    fn block_merging() {
        unsafe {
            let allocator = MmapAllocator::new();
            let layout = Layout::new::<u8>();

            // Avoid munmaping the region during the test
            allocator.alloc(Layout::new::<u64>());

            let p1 = allocator.alloc(layout);
            let p2 = allocator.alloc(layout);
            allocator.deallocate(p2);

            // After this, p1 and p2 should be merged (test: merging with next)            
            allocator.deallocate(p1);
            // This block should use the previosly merged block since p1 + p2 = 16
            let p3 = allocator.alloc(Layout::new::<u16>());
            assert_eq!(p1, p3);

            let layout2 = Layout::new::<u16>();
            let p4 = allocator.alloc(layout2);

            allocator.deallocate(p3);

            //After this, p3 and p4 should be merged (test: merging with prev)
            allocator.deallocate(p4);            

            // This block should use the previosly merged block since p3 + p4 = 32
            let p5 = allocator.alloc(Layout::new::<u32>());
            assert_eq!(p3, p5);

        }
    }

    #[test]
    fn munmap_region_when_needed() {
        unsafe {
            let allocator = MmapAllocator::new();
            let layout = Layout::new::<u64>();

            let p1 = allocator.alloc(layout);
            let p2 = allocator.alloc(layout);

            {
                // We need to use this inner scope because the mutex needs to be
                // droped so that `deallocate` can take the lock.
                let kernel = allocator.allocator.lock().unwrap();
                assert!(!kernel.regions.is_empty());
            }
            
            allocator.deallocate(p1);
            allocator.deallocate(p2);

            {
                let kernel = allocator.allocator.lock().unwrap();
                assert!(kernel.regions.is_empty());
            }

        }
    }
}