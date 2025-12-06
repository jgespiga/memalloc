//! MemAlloc is a custom implementation of a memory allocator
//! It is cross-platform, therefor we request the memory regions from the kernel using:
//! 
//! [`mmap`](https://man7.org/linux/man-pages/man2/mmap.2.html) on Unix
//! [`VirtualAlloc`](https://learn.microsoft.com/es-es/windows/win32/api/memoryapi/nf-memoryapi-virtualalloc)
//! on Windows.
//! 
//! The internal structure of the allocator looks like this:
//! 
//! ```text
//!                                     
//!
//!                     Next free block                Next free block
//!                +----------------------+  +--------------------------------------+
//!                |                      |  |                                      |
//! +--------------|----------------------|--|----+      +--------------------------|-----+
//! |        | +---|--+    +-------+    +-|--|-+  |      |        | +-------+    +--|---+ |  
//! | Region | | Free | -> | Block | -> | Free |  | ---> | Region | | Block | -> | Free | |
//! |        | +------+    +-------+    +------+  |      |        | +-------+    +------+ |
//! +---------------------------------------------+      +--------------------------------+
//!
//! ```
//! 
//! As you can see, the allocator internally keeps track of multiple regions, which inside have 
//! multiple blocks where the contents of the user are written. 
//! 
//! Additionaly, we keep track of the free blocks on each region. This is key when it comes to optimizing.
//! 
//! The main optimizations which are implemented are:
//! - **Block splitting**: we split a block to avoid wasting unnecessary space
//! - **Block merging**: we merge adjacent blocks into a bigger one
//! 
//! The main structure is [`MemAlloc`], you can follow the codebase from there.


mod list;
mod freelist;
mod block;
mod region;
mod kernel;
mod utils;
mod memalloc;


pub use memalloc::MemAlloc;