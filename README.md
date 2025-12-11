# MemAlloc

A custom, thread-safe, general-purpose memory allocator written in Rust.

MemAlloc is cross-platform and it implements the [`GlobalAlloc`](https://doc.rust-lang.org/stable/std/alloc/trait.GlobalAlloc.html) trait.

The memory is managed directly from the operating system using [`mmap`](https://man7.org/linux/man-pages/man2/mmap.2.html) syscalls on Unix and [`VirtualAlloc`](https://learn.microsoft.com/es-es/windows/win32/api/memoryapi/nf-memoryapi-virtualalloc) on Windows.

Run the examples:

```bash
cargo run --example basic
cargo run --example global
```

Run the tests:

```bash
cargo test
```

## Internal Structure

The internals of the allocator work all behind the following core Data Structures. All the source code is fully documented, including ASCII diagrams if you want further detail. For a deep dive into the codebase, the best point to start is [`src/memalloc.rs`](./src/memalloc.rs), you can follow the rest by reading the documentation and using the [intra-doc links](https://doc.rust-lang.org/rustdoc/write-documentation/linking-to-items-by-name.html).

## [Region](./src/region.rs) & [Block](./src/block.rs)

Memory is requested from the OS kernel in large chunks called **Regions**. Each region is a linked list of **Blocks**. A block contains a header (with all its metadata) and a payload (user memory).

```text
+-----------------------------------------------+
|        | +-------+    +-------+    +-------+  |
| Region | | Block | -> | Block | -> | Block |  |
|        | +-------+    +-------+    +-------+  |
+-----------------------------------------------+
```

## [FreeList](./src/freelist.rs)

To avoid iterating over every single block during allocation, we mantain a separate **Free List**.

```text
                                   Free List

                    Next free block                Next free block
               +----------------------+  +--------------------------------------+
               |                      |  |                                      |
+--------------|----------------------|--|----+      +--------------------------|-----+
|        | +---|--+    +-------+    +-|--|-+  |      |        | +-------+    +--|---+ |
| Region | | Free | -> | Block | -> | Free |  | ---> | Region | | Block | -> | Free | |
|        | +------+    +-------+    +------+  |      |        | +-------+    +------+ |
+---------------------------------------------+      +--------------------------------+
```
