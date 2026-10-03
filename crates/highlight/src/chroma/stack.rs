//! The lexer's state stack (Chroma's `LexerState.Stack`, a Go slice) as a persistent list.
//!
//! The lexer copies its stack where Chroma does not: the zero-width-loop guard
//! (`LexerState::run`) keeps every configuration the lexer had at one position, and Haxe's
//! pre-processor keeps a copy of the stack at every `#if` (a stack of stacks, itself a
//! [`Stack`]). Copies of a `Vec` cost the stack's depth, and a deep stack made tokenising
//! quadratic. Here a copy shares the nodes, `push` and `pop` touch the top only, and every
//! node records its stack's length and hash: copying and hashing a stack cost O(1), and
//! comparing two stacks costs O(1) unless they are equal (then it walks down to the nodes
//! they share).

use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// A persistent stack: clones share their nodes.
pub(crate) struct Stack<T> {
    top: Option<Rc<Node<T>>>,
}

struct Node<T> {
    value: T,
    below: Option<Rc<Node<T>>>,
    /// The length of the stack this node is the top of.
    len: usize,
    /// The hash of the stack this node is the top of.
    hash: u64,
}

/// The hash of the empty stack.
const EMPTY: u64 = 0x2545_f491_4f6c_dd1d;

/// A value's hash, for the hashes of the stacks it is in (a stand-in for `Hash` that a stack
/// computes once per push).
pub(crate) trait Fingerprint {
    fn fingerprint(&self) -> u64;
}

impl Fingerprint for String {
    /// FNV-1a.
    fn fingerprint(&self) -> u64 {
        self.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    }
}

impl<T> Fingerprint for Stack<T> {
    fn fingerprint(&self) -> u64 {
        self.top.as_ref().map_or(EMPTY, |n| n.hash)
    }
}

impl<T> Stack<T> {
    pub const fn new() -> Self {
        Self { top: None }
    }

    pub fn len(&self) -> usize {
        self.top.as_ref().map_or(0, |n| n.len)
    }

    pub fn is_empty(&self) -> bool {
        self.top.is_none()
    }

    /// The top value.
    pub fn last(&self) -> Option<&T> {
        self.top.as_ref().map(|n| &n.value)
    }

    /// Drops the top value, if any.
    pub fn pop(&mut self) {
        if let Some(top) = self.top.take() {
            self.top = match Rc::try_unwrap(top) {
                Ok(mut node) => node.below.take(),
                Err(shared) => shared.below.clone(),
            };
        }
    }

    /// Pops down to `len` values.
    pub fn truncate(&mut self, len: usize) {
        while self.len() > len {
            self.pop();
        }
    }
}

impl<T: Fingerprint> Stack<T> {
    pub fn push(&mut self, value: T) {
        let below = self.top.take();
        let (len, hash) = below.as_ref().map_or((0, EMPTY), |n| (n.len, n.hash));
        let hash = (hash.rotate_left(26) ^ value.fingerprint()).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.top = Some(Rc::new(Node {
            value,
            below,
            len: len + 1,
            hash,
        }));
    }
}

impl<T: Fingerprint> From<T> for Stack<T> {
    /// The stack of one value.
    fn from(value: T) -> Self {
        let mut s = Self::new();
        s.push(value);
        s
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for Stack<T> {
    fn clone(&self) -> Self {
        Self {
            top: self.top.clone(),
        }
    }
}

impl<T: PartialEq> PartialEq for Stack<T> {
    fn eq(&self, other: &Self) -> bool {
        let (mut a, mut b) = (&self.top, &other.top);
        loop {
            match (a, b) {
                (None, None) => return true,
                (Some(x), Some(y)) => {
                    if Rc::ptr_eq(x, y) {
                        return true;
                    }
                    if x.len != y.len || x.hash != y.hash || x.value != y.value {
                        return false;
                    }
                    (a, b) = (&x.below, &y.below);
                }
                _ => return false,
            }
        }
    }
}

impl<T: Eq> Eq for Stack<T> {}

impl<T> Hash for Stack<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.fingerprint());
        state.write_usize(self.len());
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Stack<T> {
    /// Bottom to top, like Chroma's slice.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut values = Vec::with_capacity(self.len());
        let mut node = &self.top;
        while let Some(n) = node {
            values.push(&n.value);
            node = &n.below;
        }
        values.reverse();
        f.debug_list().entries(values).finish()
    }
}

impl<T> Drop for Node<T> {
    /// Frees the nodes below that only this one holds in a loop, not recursively: stacks can be
    /// deeper than the thread's stack allows.
    fn drop(&mut self) {
        let mut below = self.below.take();
        while let Some(node) = below {
            below = match Rc::try_unwrap(node) {
                Ok(mut node) => node.below.take(),
                Err(_) => None,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(values: &[&str]) -> Stack<String> {
        let mut s = Stack::new();
        for v in values {
            s.push((*v).to_owned());
        }
        s
    }

    #[test]
    fn push_pop_truncate() {
        let mut s = stack(&["root", "a", "b"]);
        assert_eq!(s.len(), 3);
        assert_eq!(s.last().map(String::as_str), Some("b"));
        let copy = s.clone();
        s.pop();
        assert_eq!(s.last().map(String::as_str), Some("a"));
        assert_eq!(copy.len(), 3, "a copy keeps its values");
        s.truncate(0);
        assert!(s.is_empty());
        s.pop();
        assert!(s.is_empty());
        assert_eq!(format!("{copy:?}"), r#"["root", "a", "b"]"#);
    }

    #[test]
    fn equality_and_hash_follow_the_values() {
        let hash = |s: &Stack<String>| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            s.hash(&mut h);
            h.finish()
        };
        let a = stack(&["root", "a", "b"]);
        let mut b = stack(&["root", "a"]);
        assert_ne!(a, b);
        b.push("b".to_owned());
        assert_eq!(a, b);
        assert_eq!(hash(&a), hash(&b));
        assert_ne!(a, stack(&["root", "b", "a"]));
        assert_ne!(stack(&[]), stack(&["root"]));
        // Sharing the nodes below.
        let mut c = a.clone();
        c.pop();
        c.push("b".to_owned());
        assert_eq!(a, c);
        c.push("x".to_owned());
        assert_ne!(a, c);
        // A stack of stacks.
        let mut p: Stack<Stack<String>> = Stack::new();
        p.push(a.clone());
        let mut q = Stack::new();
        q.push(b);
        assert_eq!(p, q);
        q.push(a);
        assert_ne!(p, q);
    }

    #[test]
    fn deep_stacks_drop() {
        let mut s = Stack::new();
        for _ in 0..1_000_000 {
            s.push(String::new());
        }
        let copy = s.clone();
        drop(s);
        assert_eq!(copy.len(), 1_000_000);
    }
}
