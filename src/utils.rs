//! This file contains all the helper functions for the allocator. 
//! This are functions that don't particularly belong to any concrete module of the program.


/// It aligns `to_be_aligned` using `aligment`.
/// 
/// This method is used to align region sizes to be a multiple of [`crate::kernel::Kernel::page_size`]
/// and pointers in blocks to be a multiple of the computer's pointer size because memory
/// direcctions have to be aligned.
pub fn align(to_be_aligned: usize, aligment: usize) -> usize {
    (to_be_aligned + aligment - 1) & !(aligment - 1)
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn align_pointer_size() {
        let aligments = vec![(1..8, 8), (9..16, 16), (17..24, 24), (25..32, 32)];

        for (sizes, expected) in aligments {
            for size in sizes {
                assert_eq!(expected, align(size, mem::size_of::<usize>()));
            }
        }
    }

    #[test]
    fn align_page_size() {
        // For testing purposes we are assuming the page size is 4096
        let aligments = vec![(1..4096, 4096), (4097..8192, 8192)];

        for (sizes, expected) in aligments {
            for size in sizes {
                assert_eq!(expected, align(size, 4096))
            }
        }
    }
}