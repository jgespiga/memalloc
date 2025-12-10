
use std::alloc::Layout;

use memalloc::MemAlloc;

fn log_alloc(addr: *mut u8, layout: Layout) {
    println!("Requested {} bytes of memory", layout.size());
    println!("Received this address: {addr:?}");
}

fn main() {
    let allocator = MemAlloc::new();

    unsafe {

        let l1 = Layout::new::<u64>();
        let addr1 = allocator.allocate(l1);
        log_alloc(addr1, l1);

        let l2 = Layout::array::<u8>(8).unwrap();
        let addr2 = allocator.allocate(l2).cast();
        log_alloc(addr2, l2);

        let l3 = Layout::array::<u8>(16).unwrap();
        let addr3 = allocator.allocate(l3).cast();
        log_alloc(addr3, l3);

        allocator.deallocate(addr1, l1);
        allocator.deallocate(addr2, l2);
        allocator.deallocate(addr3, l3);
    }
}