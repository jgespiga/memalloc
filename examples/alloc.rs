//use std::alloc::Layout;
//use memalloc::bump_alloc::BumpAllocator;


//fn main() {
    
//    let mut allocator = BumpAllocator::new();
//
//    unsafe {
//        let layout = Layout::new::<u32>();
//        println!("Allocating memory block for 3 u32...");
//        let b1 = allocator.alloc(layout);
//        let b2 = allocator.alloc(layout);
//        let b3 = allocator.alloc(layout);
//
//        println!("Memory blocks: b1 -> {:?} ; b2 -> {:?} ; b3 -> {:?}", b1, b2, b3);
//
//        println!("Deallocating block at address {:?}", b1);
//        allocator.free(b1);
//
//        println!("Allocating memory block for 1 u64...");
//        let b4 = allocator.alloc(Layout::new::<u64>());
//        println!("Returned address -> {:?}", b4);
//
//        println!("Allocating memory block for u32, should return first deallocated address");
//        let b5 = allocator.alloc(layout);
//        println!("Returned address -> {:?}", b5);
//    }
//}

fn main() {
    return;
}