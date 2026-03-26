//! # Ruby Thread Extensions
//!
//! Mutex, ConditionVariable, Queue for threading.
//! Follows MRI's thread_sync.c structure.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar as StdCondvar, Mutex as StdMutex};

/// Ruby Mutex - mutual exclusion lock
#[repr(C)]
pub struct RubyMutex {
    /// Underlying OS mutex
    pub inner: StdMutex<MutexState>,
    /// Condition variable for waiting threads
    pub cond: StdCondvar,
}

pub struct MutexState {
    /// Current owner thread ID (0 = unlocked)
    pub owner: u64,
    /// Threads waiting for lock
    #[allow(dead_code)]
    pub waiters: VecDeque<u64>,
}

impl RubyMutex {
    /// Create a new mutex
    pub fn new() -> Self {
        Self {
            inner: StdMutex::new(MutexState {
                owner: 0,
                waiters: VecDeque::new(),
            }),
            cond: StdCondvar::new(),
        }
    }

    /// Lock the mutex
    pub fn lock(&self, thread_id: u64) {
        let mut state = self.inner.lock().unwrap();
        while state.owner != 0 && state.owner != thread_id {
            state = self.cond.wait(state).unwrap();
        }
        state.owner = thread_id;
    }

    /// Try to lock without blocking
    pub fn try_lock(&self, thread_id: u64) -> bool {
        let mut state = self.inner.lock().unwrap();
        if state.owner == 0 {
            state.owner = thread_id;
            true
        } else {
            false
        }
    }

    /// Unlock the mutex
    pub fn unlock(&self, thread_id: u64) -> bool {
        let mut state = self.inner.lock().unwrap();
        if state.owner != thread_id {
            return false;
        }
        state.owner = 0;
        self.cond.notify_one();
        true
    }

    /// Check if locked
    pub fn is_locked(&self) -> bool {
        let state = self.inner.lock().unwrap();
        state.owner != 0
    }

    /// Get owner thread ID
    pub fn owner(&self) -> u64 {
        let state = self.inner.lock().unwrap();
        state.owner
    }
}

impl Default for RubyMutex {
    fn default() -> Self {
        Self::new()
    }
}

/// Ruby ConditionVariable
#[repr(C)]
pub struct RubyCondVar {
    /// Underlying condition variable
    pub cond: StdCondvar,
}

impl RubyCondVar {
    /// Create a new condition variable
    pub fn new() -> Self {
        Self {
            cond: StdCondvar::new(),
        }
    }

    /// Wait on condition (must hold mutex)
    pub fn wait(&self, mutex: &RubyMutex) {
        // Release mutex and wait
        let guard = mutex.inner.lock().unwrap();
        let _guard = self.cond.wait(guard);
    }

    /// Wait with timeout
    pub fn wait_timeout(&self, mutex: &RubyMutex, timeout_ms: u64) -> bool {
        let guard = mutex.inner.lock().unwrap();
        let result = self.cond.wait_timeout(
            guard,
            std::time::Duration::from_millis(timeout_ms)
        );
        result.is_ok()
    }

    /// Signal one waiting thread
    pub fn signal(&self) {
        self.cond.notify_one();
    }

    /// Broadcast to all waiting threads
    pub fn broadcast(&self) {
        self.cond.notify_all();
    }
}

impl Default for RubyCondVar {
    fn default() -> Self {
        Self::new()
    }
}

/// Ruby Queue - thread-safe queue
#[repr(C)]
pub struct RubyQueue<T> {
    /// Underlying storage
    pub inner: Arc<StdMutex<QueueState<T>>>,
    /// Condition variable for blocking pop
    pub cond: Arc<StdCondvar>,
}

pub struct QueueState<T> {
    pub items: VecDeque<T>,
    pub closed: bool,
}

impl<T> RubyQueue<T> {
    /// Create a new queue
    pub fn new() -> Self {
        Self {
            inner: Arc::new(StdMutex::new(QueueState {
                items: VecDeque::new(),
                closed: false,
            })),
            cond: Arc::new(StdCondvar::new()),
        }
    }

    /// Push item to queue
    pub fn push(&self, item: T) -> Result<(), &'static str> {
        let mut state = self.inner.lock().unwrap();
        if state.closed {
            return Err("queue closed");
        }
        state.items.push_back(item);
        self.cond.notify_one();
        Ok(())
    }

    /// Pop item from queue (blocks if empty)
    pub fn pop(&self) -> Option<T> {
        let mut state = self.inner.lock().unwrap();
        loop {
            if let Some(item) = state.items.pop_front() {
                return Some(item);
            }
            if state.closed {
                return None;
            }
            state = self.cond.wait(state).unwrap();
        }
    }

    /// Try pop without blocking
    pub fn try_pop(&self) -> Option<T> {
        let mut state = self.inner.lock().unwrap();
        state.items.pop_front()
    }

    /// Get queue size
    pub fn size(&self) -> usize {
        let state = self.inner.lock().unwrap();
        state.items.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// Close the queue
    pub fn close(&self) {
        let mut state = self.inner.lock().unwrap();
        state.closed = true;
        self.cond.notify_all();
    }

    /// Check if closed
    pub fn is_closed(&self) -> bool {
        let state = self.inner.lock().unwrap();
        state.closed
    }
}

impl<T> Default for RubyQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// SizedQueue - queue with max capacity
#[repr(C)]
pub struct RubySizedQueue<T> {
    /// Underlying queue
    pub queue: RubyQueue<T>,
    /// Max capacity
    pub max: usize,
    /// Condition for blocking push when full
    pub push_cond: Arc<StdCondvar>,
}

impl<T> RubySizedQueue<T> {
    /// Create a sized queue
    pub fn new(max: usize) -> Self {
        Self {
            queue: RubyQueue::new(),
            max,
            push_cond: Arc::new(StdCondvar::new()),
        }
    }

    /// Push with blocking when full
    pub fn push(&self, item: T) {
        let mut state = self.queue.inner.lock().unwrap();
        while state.items.len() >= self.max && !state.closed {
            state = self.push_cond.wait(state).unwrap();
        }
        state.items.push_back(item);
        self.queue.cond.notify_one();
    }

    /// Pop (delegates to underlying queue)
    pub fn pop(&self) -> Option<T> {
        let result = self.queue.pop();
        self.push_cond.notify_one();
        result
    }

    /// Get max size
    pub fn max(&self) -> usize {
        self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutex_lock_unlock() {
        let m = RubyMutex::new();
        let tid = 123;
        
        m.lock(tid);
        assert!(m.is_locked());
        assert_eq!(m.owner(), tid);
        
        assert!(m.unlock(tid));
        assert!(!m.is_locked());
    }

    #[test]
    fn test_mutex_try_lock() {
        let m = RubyMutex::new();
        
        assert!(m.try_lock(1));
        assert!(!m.try_lock(2)); // Already locked
        
        m.unlock(1);
        assert!(m.try_lock(2));
    }

    #[test]
    fn test_queue_push_pop() {
        let q: RubyQueue<i32> = RubyQueue::new();
        
        q.push(1).unwrap();
        q.push(2).unwrap();
        q.push(3).unwrap();
        
        assert_eq!(q.size(), 3);
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), Some(2));
    }

    #[test]
    fn test_queue_close() {
        let q: RubyQueue<i32> = RubyQueue::new();
        q.push(1).unwrap();
        q.close();
        
        assert!(q.is_closed());
        assert!(q.push(2).is_err());
    }

    #[test]
    fn test_sized_queue() {
        let sq: RubySizedQueue<i32> = RubySizedQueue::new(2);
        
        sq.push(1);
        sq.push(2);
        assert_eq!(sq.queue.size(), 2);
        assert_eq!(sq.max(), 2);
        
        // Would block on next push if not popped
        assert_eq!(sq.pop(), Some(1));
    }
}
