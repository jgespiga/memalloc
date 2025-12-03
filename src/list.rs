use std::{marker::PhantomData, ptr::NonNull};


/// Non-null pointer to `T`.
pub(crate) type Link<T> = Option<NonNull<T>>;

pub(crate) struct Node<T> {
    /// Pointer to the next node of the list
    pub next: Link<Self>,
    /// Pointer to the previous node of the list
    pub prev: Link<Self>,
    /// Element of the node
    pub data: T,
}
pub(crate) struct List<T> {
    head: Link<Node<T>>,
    tail: Link<Node<T>>,
    len: usize,
    marker: PhantomData<T>,
}

pub struct Iter<'a, T> {
    current: Link<Node<T>>,
    remaining: usize,
    marker: PhantomData<&'a T>,
}

impl<T> Node<T> {
    pub fn new(data: T) -> Self {
        Self { next: None, prev: None, data}
    }
}

impl<T> List<T> {
    pub fn new() -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn first(&self) -> Link<Node<T>> {
        self.head
    }

    #[inline]
    pub fn last(&self) -> Link<Node<T>> {
        self.tail
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    
    /// Appends a new node to the Linked List. 
    /// 
    /// It is very important for us that, because we are the actual memory
    /// allocator, this method can not make allocations itself. Therefor, 
    /// it has to receive the `addr` where this node has to be allocated.
    /// 
    /// This way, the node will we placed inside of our data structures in
    /// the exact place we want.
    /// 
    /// **SAFETY**: Caller (we, as the allocator) must guarantee that the given `addr` is valid
    pub unsafe fn append(&mut self, data: T, addr: NonNull<u8>) -> NonNull<Node<T>> {
        let node = addr.cast::<Node<T>>();
        
        unsafe {
            node.as_ptr().write(Node {
                next: None,
                prev: self.tail,
                data,
            });

            if let Some(mut tail) = self.tail {
                tail.as_mut().next = Some(node);
            } else {
                self.head = Some(node);
            }

            self.tail = Some(node);
            self.len += 1;

            node
        }
    }

    /// Inserts a new block right after the given `node` in the list. 
    /// 
    /// **SAFETY**: Caller must guarantee that `node` is an actual block of the list.
    pub unsafe fn insert_after(
        &mut self, 
        mut node: NonNull<Node<T>>, 
        data: T, 
        addr: NonNull<u8>
    ) -> NonNull<Node<T>> {
        let new = addr.cast::<Node<T>>();

        unsafe {
            let next = node.as_mut().next;

            new.as_ptr().write(Node {
                prev: Some(node),
                next,
                data,
            });

            node.as_mut().next = Some(new);

            if let Some(mut next_node) = next {
                next_node.as_mut().prev = Some(new);
            } else {
                self.tail = Some(new);
            }

            self.len += 1;
            new
        }
    } 

    pub unsafe fn remove(&mut self, node: NonNull<Node<T>>) {
        unsafe {
            let prev = node.as_ref().prev;
            let next = node.as_ref().next;

            // Link prev -> next
            if let Some(mut prev_node) = prev {
                prev_node.as_mut().next = next;
            } else {
                self.head = next;
            }

            // Link next -> prev
            if let Some(mut next_node) = next {
                next_node.as_mut().prev = prev;
            } else {
                // Node was the tail
                self.tail = prev;
            }
        }
            
        self.len -= 1;
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            current: self.head,
            remaining: self.len,
            marker: PhantomData,
        }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.current?;

        unsafe {
            self.current = node.as_ref().next;
            self.remaining -= 1;

            Some(&node.as_ref().data)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{alloc, dealloc, Layout};

    // The problem with testing this [`List`] is that, as detailed avobe, as we
    // are the allocator we cannot make allocations by ourselves. Because of that,
    // we need to simulate what our allocator will do in order to have a valid address
    // we can give to each node on our linked list.
    // 
    // Therefor, we will use `std::alloc` and `std::dealloc` to test our list.

    /// Helper function to get a new memory address for a new node
    unsafe fn get_memory_for_node<T>() -> NonNull<u8> {
        unsafe {
            let layout = Layout::new::<Node<T>>();
            let ptr = alloc(layout);
            if ptr.is_null() {
                panic!("Falló la asignación de memoria para el test");
            }
            NonNull::new_unchecked(ptr)
        }
    }

    /// Helper to clean up the memory allocated for the `node`
    unsafe fn clean_up_node<T>(node: NonNull<Node<T>>) {
        unsafe {
            let ptr = node.as_ptr() as *mut u8;
            let layout = Layout::new::<Node<T>>();
            dealloc(ptr, layout);
        }
    }

    #[test]
    fn new_list_is_empty() {
        let list: List<i32> = List::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        assert!(list.head.is_none());
        assert!(list.tail.is_none());
    }

    #[test]
    fn append_single_element() {
        unsafe {
            let mut list = List::<i32>::new();
            let mem = get_memory_for_node::<i32>();
            
            list.append(42, mem);

            assert_eq!(list.len(), 1);
            assert!(!list.is_empty());
            
            assert_eq!(list.head, list.tail);
            
            assert_eq!(list.head.unwrap().as_ref().data, 42);
            
            clean_up_node(list.head.unwrap());
        }
    }

    #[test]
    fn append_multiple_elements_and_iter() {
        unsafe {
            let mut list = List::<i32>::new();
            
            let nodes_data = vec![1, 2, 3];
            let mut node_ptrs = Vec::new();

            for &data in &nodes_data {
                let mem = get_memory_for_node::<i32>();
                let node = list.append(data, mem);
                node_ptrs.push(node);
            }

            assert_eq!(list.len(), 3);

            let collected: Vec<&i32> = list.iter().collect();
            assert_eq!(collected, vec![&1, &2, &3]);

            let node1 = node_ptrs[0].as_ref();
            let node2 = node_ptrs[1].as_ref();
            let node3 = node_ptrs[2].as_ref();

            // 1 -> 2
            assert_eq!(node1.next, Some(node_ptrs[1]));
            assert_eq!(node1.prev, None);

            // 1 <- 2 -> 3
            assert_eq!(node2.prev, Some(node_ptrs[0]));
            assert_eq!(node2.next, Some(node_ptrs[2]));

            // 2 <- 3
            assert_eq!(node3.prev, Some(node_ptrs[1]));
            assert_eq!(node3.next, None);

            for node in node_ptrs {
                clean_up_node(node);
            }
        }
    }

    #[test]
    fn remove_head() {
        unsafe {
            let mut list = List::<i32>::new();
            let n1 = list.append(10, get_memory_for_node::<i32>());
            let n2 = list.append(20, get_memory_for_node::<i32>());

            assert_eq!(list.len(), 2);

            list.remove(n1);

            assert_eq!(list.len(), 1);
            assert_eq!(list.head, Some(n2));
            assert_eq!(list.tail, Some(n2));
            assert_eq!(n2.as_ref().prev, None);

            clean_up_node(n1);
            clean_up_node(n2);
        }
    }

    #[test]
    fn remove_tail() {
        unsafe {
            let mut list = List::<i32>::new();
            let n1 = list.append(10, get_memory_for_node::<i32>());
            let n2 = list.append(20, get_memory_for_node::<i32>());

            list.remove(n2);

            assert_eq!(list.len(), 1);
            assert_eq!(list.tail, Some(n1));
            assert_eq!(list.head, Some(n1));
            assert_eq!(n1.as_ref().next, None);

            clean_up_node(n1);
            clean_up_node(n2);
        }
    }

    #[test]
    fn remove_middle() {
        unsafe {
            let mut list = List::<i32>::new();
            let n1 = list.append(10, get_memory_for_node::<i32>());
            let n2 = list.append(20, get_memory_for_node::<i32>());
            let n3 = list.append(30, get_memory_for_node::<i32>());

            
            list.remove(n2);

            assert_eq!(list.len(), 2);
            
            assert_eq!(n1.as_ref().next, Some(n3));
            assert_eq!(n3.as_ref().prev, Some(n1));

            clean_up_node(n1);
            clean_up_node(n2);
            clean_up_node(n3);
        }
    }

    #[test]
    fn test_insert_after() {
        unsafe {
            let mut list = List::<i32>::new();
            
            let n1 = list.append(10, get_memory_for_node::<i32>());
            
            let n2 = list.insert_after(n1, 20, get_memory_for_node::<i32>());
            
            assert_eq!(list.len(), 2);
            assert_eq!(list.tail, Some(n2));
            assert_eq!(n1.as_ref().next, Some(n2));
            assert_eq!(n2.as_ref().prev, Some(n1));

            let n1_5 = list.insert_after(n1, 15, get_memory_for_node::<i32>());

            assert_eq!(list.len(), 3);
            
            let vec: Vec<&i32> = list.iter().collect();
            assert_eq!(vec, vec![&10, &15, &20]);

            // 10 -> 15
            assert_eq!(n1.as_ref().next, Some(n1_5));
            // 15 -> 20
            assert_eq!(n1_5.as_ref().next, Some(n2));
            // 15 <- 20
            assert_eq!(n2.as_ref().prev, Some(n1_5));

            clean_up_node(n1);
            clean_up_node(n1_5);
            clean_up_node(n2);
        }
    }

    #[test]
    fn remove_last_remaining_node() {
        unsafe {
            let mut list = List::<i32>::new();
            let n1 = list.append(99, get_memory_for_node::<i32>());

            list.remove(n1);

            assert!(list.is_empty());
            assert!(list.head.is_none());
            assert!(list.tail.is_none());

            clean_up_node(n1);
        }
    }
}