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

    pub unsafe fn remove(&mut self, mut node: NonNull<Node<T>>) {
        unsafe {
            if self.len == 1 {
                self.head = None;
                self.tail = None;
            } else if node == self.head.unwrap() {
                node.as_mut().prev.unwrap().as_mut().prev = None;
                self.head = node.as_ref().next;
            } else if node == self.tail.unwrap() {
                node.as_mut().prev.unwrap().as_mut().next = None;
                self.tail = node.as_ref().prev;
            } else {
                let mut next = node.as_ref().next.unwrap();
                let mut prev = node.as_ref().prev.unwrap();
                prev.as_mut().next = Some(next);
                next.as_mut().prev = Some(prev);
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

    #[test]
    fn new_list_is_empty() {
        let list: List<u8> = List::new();

        assert_eq!(list.len, 0);
        assert!(list.is_empty());
        assert!(list.iter().next().is_none());
    }
}