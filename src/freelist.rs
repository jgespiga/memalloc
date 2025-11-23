use std::ptr::NonNull;

use crate::{list::{List, Node}, mmap::Block, region::Region};

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
pub(crate) struct FreeList {
    pub items: List<NonNull<Node<Block>>>
}

impl FreeList {
    /// Creates a new empty List
    pub fn new() -> Self {
        return Self { items: List::new() }
    }
    
    /// It tells whether the FreeList is empty or not.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    /// Inserts an existing `block` into the FreeList.
    /// Because this [`FreeList`] is an abstraction built over [`List`] we
    /// need to give this method the `addr` where the node is going to be written.
    /// 
    /// For more information about this decision see [`List::append`] 
    pub fn insert_free_block(&mut self, block: NonNull<Node<Block>>, addr: NonNull<u8>) -> NonNull<Node<NonNull<Node<Block>>>> {
        unsafe { self.items.append(block, addr) }
    }

    /// Removes a `node` from the FreeList. 
    /// 
    /// See [`List::remove`] for more detail about how this works.
    pub fn remove_free_block(&mut self, node: NonNull<Node<NonNull<Node<Block>>>>) {
        unsafe { self.items.remove(node); }
    } 

    // TODO: Should insert and remove change the `is_free` flag on the given block?
}