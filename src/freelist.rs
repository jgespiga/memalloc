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
/// 
/// 
/// Additionaly, we are going to use the payload of every free block as storage to keep
/// the metadata we introduce by keeping a list of free blocks. We use this approach since,
/// as the block is actually free, the only part of it that we need is its header but the
/// payload is actually empty and won't be used by the user:
/// 
/// ```text
/// +------------------------+ <--------+ 
/// |       Node<Block>      |          |
/// +------------------------+          |
/// |       Block.data:      |          |-------> Block Header
/// |        - size          |          |
/// |        - is_free       |          |
/// +------------------------+ <--------+
/// |                        |
/// |      Free Payload      | 
/// |        (unused)        |
/// |                        |
/// +------------------------+
/// ```
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
    pub fn insert_free_block(&mut self, mut block: NonNull<Node<Block>>, addr: NonNull<u8>) -> NonNull<Node<NonNull<Node<Block>>>> {
        unsafe {
            // Mark the block as free to use
            block.as_mut().data.is_free = true;
            
            // Add the block from the list
            self.items.append(block, addr) 
        }
    }

    /// Removes a `node` from the FreeList. 
    /// 
    /// ### Notes
    /// The extra logic here is needed because [`FreeList`] is a LinkedList of
    /// pointers but, we are given a block we want to remove since that's the "high-level"
    /// view the allocator has on the block that it wants to take.
    /// 
    /// 
    /// **SAFETY**: Caller must guarantee that node is actually a free block
    /// 
    /// See [`List::remove`] for more detail about how the actual removal works.
    pub fn remove_free_block(&mut self, mut node: NonNull<Node<Block>>) {
        let mut current = self.items.first();

        while let Some(free_node) = current {
            unsafe {
                if free_node.as_ref().data == node {
                    // We found the block in the FreeList

                    // Mark the block as used
                    node.as_mut().data.is_free = false;

                    // Remove the block from the list
                    self.items.remove(free_node);
                    return;
                }

                current = free_node.as_ref().next;
            }
        }
    } 
}