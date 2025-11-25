use std::{alloc::Layout, ptr::NonNull};
use memalloc::mmap::MmapAllocator;

fn log_alloc(addr: *mut u8, layout: Layout) {
    println!("Requested {} bytes of memory", layout.size());
    println!("Received this address: {addr:?}");
}
fn main() {
    unsafe {
        let mut allocator = MmapAllocator::new();
    
        let layout1 = Layout::new::<u8>();
        let addr1 = allocator.alloc(layout1);
        log_alloc(addr1, layout1);

        let layout2 = Layout::array::<u8>(1024).unwrap();
        let addr2 = allocator.alloc(layout2);
        log_alloc(addr2, layout2);

        let layout3 = Layout::array::<u8>(4096).unwrap();
        let addr3 = allocator.alloc(layout3);
        log_alloc(addr3, layout3);

        println!("Deallocating everything...");
        allocator.dealloc(addr1);
        allocator.dealloc(addr2);
        allocator.dealloc(addr3);
    }
}


