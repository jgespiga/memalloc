# MemAlloc

Memory allocator written in Rust for learning purposes. The memory is requested from the kernel using [`mmap`](https://man7.org/linux/man-pages/man2/mmap.2.html) syscalls.

Run the examples:

```bash
cargo run --example mmap_alloc
```

Run the tests:

```bash
cargo test
```

## Allocator's structure

The internals of the allocator work all behind the following Data Structures. All the source code is documented including ASCII diagrams if you want further detail.

## Regions & Blocks

```text
+-----------------------------------------------+
|        | +-------+    +-------+    +-------+  |
| Region | | Block | -> | Block | -> | Block |  |
|        | +-------+    +-------+    +-------+  |
+-----------------------------------------------+
```

## Free List

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
