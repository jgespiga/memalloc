
use std::sync::LazyLock;

use memalloc::MemAlloc;

#[global_allocator]
static ALLOCATOR: MemAlloc = MemAlloc::new();