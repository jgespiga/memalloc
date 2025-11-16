use std::alloc::Layout;
use memalloc::mmap::MmapAllocator;


fn main() {
    unsafe {
        let mut allocator = MmapAllocator::new();
    
        let layout = Layout::new::<u32>();
        // Allocated space for unsigned 32 bit integer.
        let block1 = allocator.alloc(layout);
        println!("{:?}", block1);
        let block2 = allocator.alloc(layout);
        println!("{:?}", block2);

        println!("Deallocating block1");
        allocator.dealloc(block1);
        
        let block3 = allocator.alloc(layout);
        println!("Should be first addr {:?}", block3);
    }
}


