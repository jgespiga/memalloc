//! This example is designed to test the implementation of 
//! the trait `GlobalAlloc` on our allocator. See [`MemAlloc`] to
//! see the actual trait implementation.

use memalloc::MemAlloc;
use std::{thread};

#[global_allocator]
static ALLOCATOR: MemAlloc = MemAlloc::new();

fn main() {

    // Box example
    let val_box = Box::new(22);
    println!("Box Value: {}, At: {:p}", val_box, val_box);

    // Vec example
    let mut v = Vec::new();
    for i in 0..5 {
        v.push(i * 10);
        println!("Added {}; Capacity: {}; At: {:p}", 
            v[i], v.capacity(), v.as_ptr());
    }

    // String example
    let msg = String::from("Heap Testing");
    println!("\nString '{}' - At: {:p}", msg, msg.as_ptr());

    let p1 = Box::new(2.22);
    let addr_p1 = format!("{:p}", p1);
    println!("P1 Allocated at: {}", addr_p1);
    
    drop(p1);     
    println!("P1 Deallocated");

    let p2 = Box::new(2.22);
    let addr_p2 = format!("{:p}", p2);
    println!("P2 at: {}", addr_p2);


    // Merge example
    let a = Box::new([0u8; 64]);
    let b = Box::new([0u8; 64]);
    let ptr_a = a.as_ptr(); 

    drop(a);
    drop(b); 

    let c = Box::new([0u8; 128]); 
    let ptr_c = c.as_ptr();

    if ptr_a == ptr_c {
        println!("Correctly reused at {:p}", ptr_c);
    } else {
        println!("Not correctly reused. A was at {:p} and C is at {:p}", ptr_a, ptr_c);
    }

    // Thread example test
    let t1 = thread::spawn(|| {
        let _ = Box::new(222);
    });

    let t2 = thread::spawn(|| {
        let _ = Box::new(222);
    });

    t1.join().unwrap();
    t2.join().unwrap();
}