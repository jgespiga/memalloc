use std::{mem, ptr::NonNull};
use crate::{block::{BLOCK_HEADER_SIZE, Block}, freelist::FreeList, list::{List, Node}};


/// This is the overhead size introduced by the [`Region`] header in bytes.
/// The header is represented as a [`Node`]. See [`List`] for more detail.
pub(crate) const REGION_HEADER_SIZE: usize = mem::size_of::<Node<Region>>();

/// This struct contains the memory regions specific metadata. However,
/// as every other header, this is usually represented as a [`Node<Region>`]
/// so that would be the complete region data.
/// 
/// [`libc::mmap`] gives as memory regions aligned with the computer page size. 
/// But, we cannot use a full Region each time user allocates memory since we 
/// will be wasting a lot of  space. Also, we cannot assume this regions are adjacent.
/// 
/// Therefor, we are going to use the following data structure which consists in 
/// a LinkedList of [`Region`] which inside of them have a LinkedList of [`Block`].
/// 
/// ```text
/// +-----------------------------------------------+      +-----------------------------------------------+
/// |        | +-------+    +-------+    +-------+  |      |        | +-------+    +-------+    +-------+  |
/// | Region | | Block | -> | Block | -> | Block |  | ---> | Region | | Block | -> | Block | -> | Block |  |
/// |        | +-------+    +-------+    +-------+  |      |        | +-------+    +-------+    +-------+  |
/// +-----------------------------------------------+      +-----------------------------------------------+
/// ```
pub struct Region {
    /// Size of the region
    pub size: usize,
    /// List of blocks in the region
    pub blocks: List<Block>,
}


impl Region {
    /// Tries to merge the given block `node` with the previous one
    /// on the list. This can be performed if that previos block is free.
    pub(crate) fn merge_with_prev(&mut self, node: &mut NonNull<Node<Block>>, free_list: &mut FreeList) {
        unsafe {
            let block = &mut node.as_mut().data;

            // If the previous block is free, we can merge it with this one.
            if let Some(mut prev_node) = node.as_ref().prev {
                let prev_block = &mut prev_node.as_mut().data;

                if prev_block.is_free {
                    // As prev_block is already in the `free list` we just need to increment its size
                    // and remove its adjacent block with which we are going to merge this one from the list

                    // We extract the previous one from the free_list temporarily, this avoids corruption problems.
                    free_list.remove_free_block(prev_node);

                    // We need to cover the header and the actual content of the block
                    prev_block.size += BLOCK_HEADER_SIZE + block.size;
                    
                    // We remove the block from the list since it is going to be merged
                    self.blocks.remove(*node);

                    // The current block is now its previous one
                    *node = prev_node;
                }
            }
        }
    }

    /// Tries to merge the given block `node` with the next one on the
    /// list. This can be performed if that next block is free.
    pub(crate) fn merge_with_next(&mut self, node: &mut NonNull<Node<Block>>, free_list: &mut FreeList) {
        unsafe {
            if let Some(mut next_node) = node.as_ref().next {
                let next_block = &mut next_node.as_mut().data;

                if next_block.is_free {
                    // The current block should already be on the free_list, so we just need to absorb the next one.
                    free_list.remove_free_block(next_node);

                    node.as_mut().data.size += BLOCK_HEADER_SIZE + next_block.size;
                    // We remove the block from the list since it is going to be merged                   
                    self.blocks.remove(next_node);
               }
            }
        }
    }

}