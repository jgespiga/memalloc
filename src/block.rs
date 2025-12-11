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
/// +---------------------+ <------+        
/// |        size         |        |
/// +---------------------+        |
/// |    is_free (1b)     |        | -> Header
/// +---------------------+        |
/// |       region        |        |
/// +---------------------+ <------+
/// |     Additional      |
/// |      metadata       |
/// +---------------------+ <------+                    
/// |       Content       |        |
/// |         ...         |        | 
/// |         ...         |        | -> Addressable content
/// |         ...         |        |
/// |                     |        |
/// +---------------------+ <------+
/// ```
/// 
/// As always take we need to take into account that every memory address needs to be
/// aligned for CPU efficiency. The reason behind this is that processors fetch data in
/// fixed-size chunks (depending on the word's size of the computer) rather than byte by byte.
/// Therefor, we need to have this in mind every time we create a new block. When we add the 
/// Header to the content we need to align that to the computer's word size.
/// 
/// See [`crate::utils::align`] to see how this is done.
/// 
/// As a side note, take into account that to keep every address alignment, blocks usually have
/// a `padding` between the header and the content. This is the way to ensure that every used
/// address is aligned. But after doing that, we face a problem: now we don't know where is the 
/// actual `header` of the block since every block will have a different padding. To fix this,
/// we will used a little bit of space just before the block's content to store a pointer to
/// the header's address. By doing that, we can always locate the `header` by using that information.
/// However that is also what can cause Undefined Behaviour since that pointer can have a memory address
/// that isn't actually a `header`.
pub(crate) struct Block {
    /// Size of the block.
    pub size: usize, 
    /// Flag to tell whether the block is free or not.
    pub is_free: bool,
    /// Region which the block belongs to
    pub region: NonNull<Node<Region>>,
}