use std::{alloc::Layout, mem, ptr};
use libc::{intptr_t, sbrk, c_void};

/// Block header. Contains metadata about the allocated block.
struct Header {
    // Size of the memory block.
    size: usize,
    // Flag to tell block status.
    is_free: bool,
    // Next block in the list.
    next: *mut Header,
}

/// Every allocated block has an associated header with metadata that precedes the actual
/// memory block, therefore 
/// +-------------------------------+
/// | Header   | Actual memoy block |
/// +-------------------------------+
/// 
/// The returned pointer is at the start of the memory block.


/// Linked list used to store memory blocks.
pub struct BumpAllocator {
    first: *mut Header,
    last: *mut Header,
}

fn align(to_be_aligned: usize) -> usize{
    (to_be_aligned + mem::size_of::<usize>() - 1) & !(mem::size_of::<usize>() - 1)
} 

impl BumpAllocator {
    pub fn new() -> Self {
        Self { 
            first: ptr::null_mut(), 
            last: ptr::null_mut()  
        }
    }

    unsafe fn find_free_block(&self, size: usize) -> *mut Header {
        let mut current: *mut Header = self.first;

        unsafe {
            while !current.is_null() {
                if (*current).size >= size && (*current).is_free {
                    return current;
                }
                current = (*current).next;
            }
        }

        ptr::null_mut()
    }

    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        unsafe {
            let free_block = self.find_free_block(layout.size());
            if !free_block.is_null() {
                (*free_block).is_free = false;
                return (free_block as *mut u8).add(mem::size_of::<Header>());
            }
        }

        // If we did not find any free blocks, we need to increment the program
        // break in order to allocate a new block

        // We align memory size for faster access.
        let total_size = align(mem::size_of::<Header>() + layout.size());
        unsafe { 
            let addr: *mut c_void = sbrk(total_size as intptr_t); 
            
            if addr == usize::MAX as *mut c_void {
                return ptr::null_mut();
            }
            let header = addr as *mut Header;
            
            (*header).size = layout.size();
            (*header).is_free = false;
            (*header).next = ptr::null_mut();
        
            
            if self.first.is_null() {
                self.first = header;
                self.last = header;
            } else {
                (*self.last).next = header;
                self.last = (*self.last).next;
            }
            
            return (addr as *mut u8).add(mem::size_of::<Header>()); 
        }
    }

    pub unsafe fn free(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }

        unsafe {
            let header = ptr.sub(mem::size_of::<Header>()) as *mut Header;

            // Mark the block as free to use.
            (*header).is_free = true;

            // If the block is not the last block on the list, we can't do anything
            // since we cannot remove intermediate blocks.
            if header != self.last {
                return;
            }

            if self.first == self.last {
                self.first = ptr::null_mut();
                self.last = ptr::null_mut();
            } else {
                let mut current = self.first;

                while !((*current).next).is_null() && (*current).next != self.last {
                    current = (*current).next;
                }
                self.last = current;
            }

            sbrk((0 - (*header).size - mem::size_of::<Header>()) as intptr_t);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_size() {
        let aligments = vec![(1..8, 8), (9..16, 16), (17..24, 24), (25..32, 32)];

        for (sizes, expected) in aligments {
            for size in sizes {
                assert_eq!(expected, align(size));
            }
        }
    }

    #[test]
    fn basic_alloc() {
        let mut allocator = BumpAllocator::new();
        unsafe {
            let layout = Layout::new::<u32>();
            // Allocated space for unsigned 32 bit integer.
            let block = allocator.alloc(layout);
            *block = 23;
            assert_eq!(23, *block);
        }
    }

    #[test]
    fn space_for_free_block_is_used() {
        let mut allocator = BumpAllocator::new();
        unsafe {
            let first_block = allocator.alloc(Layout::new::<u32>());
            let _ = allocator.alloc(Layout::new::<u64>());
            let _ = allocator.alloc(Layout::new::<u64>());

            allocator.free(first_block);

            let second_block = allocator.alloc(Layout::new::<u32>());

            assert_eq!(first_block, second_block);
        }
    }
}