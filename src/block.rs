use std::{ptr::NonNull, mem};
use crate::{list::Node, region::Region};


/// Header size of a block. We need to add the overhead introduced by our 
/// [`Node`] structure since we always use our `Block` as a node of our linked list.
pub(crate) const BLOCK_HEADER_SIZE: usize = mem::size_of::<Node<Block>>();

/// This is the structure of a block. The fields of the block are it's metadata,
/// content is placed after this header.
/// 
/// The following diagram represents this structure ignoring that the block will be 
/// wrapped inside a [`Node`]
/// 
/// ```text
/// +----------------+        +
/// |      size      |        |
/// +----------------+        |
/// |   is_free (1b) |        | -> Header
/// +----------------+        |
/// |     region     |        |
/// +----------------+        +       
/// |     Content    |
/// |                |
/// +----------------+
/// ```
pub(crate) struct Block {
    /// Size of the block.
    pub size: usize, 
    /// Flag to tell whether the block is free or not.
    pub is_free: bool,
    /// Region which the block belongs to
    pub region: NonNull<Node<Region>>,
}