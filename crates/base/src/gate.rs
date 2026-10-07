//! [`Gate`]: at most a given number of threads at once in a section of code; the others wait.
//!
//! File systems serialise changes to a directory (and macOS's also lookups), so many threads
//! creating files at once mostly wait for each other in the kernel: a build writes its
//! publish directory through a gate of a few threads, and is faster for it.

use std::sync::{Condvar, Mutex, PoisonError};

/// A counting semaphore.
#[derive(Debug)]
pub struct Gate {
    inside: Mutex<usize>,
    left: Condvar,
    limit: usize,
}

/// A thread inside a [`Gate`]: it leaves when the pass is dropped.
#[must_use = "the thread leaves the gate when the pass is dropped"]
#[derive(Debug)]
pub struct Pass<'a>(&'a Gate);

impl Gate {
    /// A gate that lets `limit` threads in at once (at least one).
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Self {
            inside: Mutex::new(0),
            left: Condvar::new(),
            limit: if limit == 0 { 1 } else { limit },
        }
    }

    /// Waits until fewer than the limit are inside, then enters.
    pub fn enter(&self) -> Pass<'_> {
        let mut inside = self.inside.lock().unwrap_or_else(PoisonError::into_inner);
        while *inside >= self.limit {
            inside = self
                .left
                .wait(inside)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *inside += 1;
        Pass(self)
    }
}

impl Drop for Pass<'_> {
    fn drop(&mut self) {
        *self.0.inside.lock().unwrap_or_else(PoisonError::into_inner) -= 1;
        self.0.left.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn at_most_the_limit_inside() {
        let gate = Gate::new(2);
        let (inside, most) = (AtomicUsize::new(0), AtomicUsize::new(0));
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..50 {
                        let _pass = gate.enter();
                        let n = inside.fetch_add(1, Ordering::SeqCst) + 1;
                        most.fetch_max(n, Ordering::SeqCst);
                        std::thread::yield_now();
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                });
            }
        });
        assert!(most.load(Ordering::SeqCst) <= 2);
        assert_eq!(*gate.inside.lock().unwrap(), 0);
    }
}
