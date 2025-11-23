use std::{mem};

use crate::{list::{List, Node}, mmap::Block};


/// This is the overhead size introduced by the [`Region`] header in bytes.
/// The header is represented as a [`Node`]. See [`List`] for more detail.
pub(crate) const REGION_HEADER_SIZE: usize = mem::size_of::<Node<Region>>();

/// This struct contains the memory regions specific metadata. However,
/// as every other header, this is usually represented as a [`Node<Region>`]
/// so that would be the complete region data.
/// 
/// [`libc::mmap`] gives as memory regions aligned with the computer
/// page size. But, we cannot use a full Region each time
/// user allocates memory since we will be wasting a lot of 
/// space. Also, we cannot assume this regions are adjacent.
/// 
/// Therefor, we are going to use the following data structure
/// which consists in a LinkedList of [`Region`] which inside of them
/// have a LinkedList of [`Block`].
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
    size: usize,
    /// List of blocks in the region
    blocks: List<Block>,
}