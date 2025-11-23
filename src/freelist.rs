use std::ptr::NonNull;

use crate::{list::{List, Node}, mmap::Block, region::{Region}};

/// Linked list to keep track of free [`Block`]. 
/// 
/// This list only stores pointers to the actual [`Region`] blocks. The reason behind 
/// this is that we don't actually need to store any additional content for blocks which 
/// are free. We just need to keep track of them.
/// 
/// ```text
/// 
///    Free Block                   Next Free Block
/// 
///         +------------------------------+
///         |                              |
/// +-------|------+               +-------|------+
/// |  Block(free) |               |  Block(free) | 
/// +--------------+               +--------------+
/// 
/// ```
/// 
/// Inside of the actual allocator, this will look something like this:
/// 
/// ```text
///                                     Free List
/// 
///                     Next free block                Next free block
///                +----------------------+  +--------------------------------------+
///                |                      |  |                                      |
/// +--------------|----------------------|--|----+      +--------------------------|-------------------+
/// |        | +---|--+    +-------+    +-|--|-+  |      |        | +-------+    +--|---+    +-------+  |
/// | Region | | Free | -> | Block | -> | Free |  | ---> | Region | | Block | -> | Free | -> | Block |  |
/// |        | +------+    +-------+    +------+  |      |        | +-------+    +------+    +-------+  |
/// +---------------------------------------------+      +----------------------------------------------+
/// 
/// ```
/// 
/// All the free blocks can be identified by the [`Block::is_free`] flag and, as allways,
/// all block headers are of type [`Node<Block>`], so thats were we are pointing to.
type FreeList = List<NonNull<Node<Block>>>;