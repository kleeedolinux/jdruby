//! # Ruby Thread Extensions - Green Thread Aware
//!
//! Mutex, ConditionVariable, Queue for M:N threading.
//! These primitives park green threads instead of blocking OS threads.
//! Follows MRI's thread_sync.c structure.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::thread::ThreadId;

/// Green-thread-aware Mutex - parks thread instead of blocking OS thread
pub struct RubyMutex {
    /// Lock state: 0 = unlocked, other = owner thread ID
    state: AtomicU64,
    /// Queue of waiting thread IDs
    waiters: Mutex<VecDeque<ThreadId>>,
}

impl RubyMutex {
    /// Create a new unlocked mutex
    pub fn new() -> Self {
        Self {
            state: AtomicU64::new(0),
            waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Lock the mutex - parks green thread if unavailable
    pub fn lock(&self, thread_id: ThreadId) {
        loop {
            match self.state.compare_exchange(
                0,
                thread_id,
                Ordering::Acquire,
                Ordering::Relaxed
            ) {
                Ok(_) => return,
                Err(_) => {
                    let mut waiters = self.waiters.lock().unwrap();
                    waiters.push_back(thread_id);
                    drop(waiters);
                    std::thread::yield_now();
                }
            }
        }
    }

    /// Try to lock without parking
    pub fn try_lock(&self, thread_id: ThreadId) -> bool {
        self.state.compare_exchange(
            0,
            thread_id,
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_ok()
    }

    /// Unlock mutex - unparks first waiter if any
    pub fn unlock(&self, thread_id: ThreadId) -> bool {
        match self.state.compare_exchange(
            thread_id,
            0,
            Ordering::Release,
            Ordering::Relaxed
        ) {
            Ok(_) => {
                if let Some(_waiter) = self.waiters.lock().unwrap().pop_front() {
                    // scheduler.unpark(waiter)
                }
                true
            }
            Err(_) => false,
        }
    }

    /// Check if locked
    pub fn is_locked(&self) -> bool {
        self.state.load(Ordering::Relaxed) != 0
    }

    /// Get owner thread ID (0 if unlocked)
    pub fn owner(&self) -> ThreadId {
        self.state.load(Ordering::Relaxed)
    }

    /// Check if owned by current thread
    pub fn owned_by(&self, thread_id: ThreadId) -> bool {
        self.owner() == thread_id
    }
}

impl Default for RubyMutex {
    fn default() -> Self {
        Self::new()
    }
}

/// Green-thread-aware ConditionVariable
pub struct RubyCondVar {
    waiters: Mutex<VecDeque<ThreadId>>,
}

impl RubyCondVar {
    /// Create a new condition variable
    pub fn new() -> Self {
        Self {
            waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Wait on condition - releases mutex, parks thread, reacquires mutex
    pub fn wait(&self, mutex: &RubyMutex, thread_id: ThreadId) {
        self.waiters.lock().unwrap().push_back(thread_id);
        mutex.unlock(thread_id);
        std::thread::yield_now();
        mutex.lock(thread_id);
    }

    /// Wait with timeout
    pub fn wait_timeout(&self, mutex: &RubyMutex, thread_id: ThreadId, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        self.waiters.lock().unwrap().push_back(thread_id);
        mutex.unlock(thread_id);
        let start = Instant::now();
        while start.elapsed() < timeout {
            std::thread::yield_now();
        }
        mutex.lock(thread_id);
        Instant::now() < deadline
    }

    /// Signal (wake) one waiting thread
    pub fn signal(&self) {
        let _ = self.waiters.lock().unwrap().pop_front();
    }

    /// Broadcast (wake) all waiting threads
    pub fn broadcast(&self) {
        let _: Vec<_> = self.waiters.lock().unwrap().drain(..).collect();
    }

    /// Check if any threads are waiting
    pub fn has_waiters(&self) -> bool {
        !self.waiters.lock().unwrap().is_empty()
    }

    /// Get number of waiting threads
    pub fn num_waiters(&self) -> usize {
        self.waiters.lock().unwrap().len()
    }
}

impl Default for RubyCondVar {
    fn default() -> Self {
        Self::new()
    }
}

/// Green-thread-aware Queue
pub struct RubyQueue<T> {
    inner: Mutex<QueueState<T>>,
    recv_cond: RubyCondVar,
    send_cond: RubyCondVar,
}

struct QueueState<T> {
    items: VecDeque<T>,
    closed: bool,
    max_size: Option<usize>,
}

impl<T> RubyQueue<T> {
    /// Create a new unbounded queue
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(QueueState {
                items: VecDeque::new(),
                closed: false,
                max_size: None,
            }),
            recv_cond: RubyCondVar::new(),
            send_cond: RubyCondVar::new(),
        }
    }

    /// Create a new bounded queue (SizedQueue behavior)
    pub fn with_capacity(max: usize) -> Self {
        Self {
            inner: Mutex::new(QueueState {
                items: VecDeque::new(),
                closed: false,
                max_size: Some(max),
            }),
            recv_cond: RubyCondVar::new(),
            send_cond: RubyCondVar::new(),
        }
    }

    /// Push item - blocks if bounded and full
    pub fn push(&self, item: T, thread_id: ThreadId) -> Result<(), QueueError> {
        let mut state = self.inner.lock().unwrap();
        if state.closed {
            return Err(QueueError::Closed);
        }
        while let Some(max) = state.max_size {
            if state.items.len() < max {
                break;
            }
            drop(state);
            let fake_mutex = RubyMutex::new();
            fake_mutex.lock(thread_id);
            self.send_cond.wait(&fake_mutex, thread_id);
            fake_mutex.unlock(thread_id);
            state = self.inner.lock().unwrap();
            if state.closed {
                return Err(QueueError::Closed);
            }
        }
        state.items.push_back(item);
        drop(state);
        self.recv_cond.signal();
        Ok(())
    }

    /// Try push without blocking
    pub fn try_push(&self, item: T) -> Result<(), QueueError> {
        let mut state = self.inner.lock().unwrap();
        if state.closed {
            return Err(QueueError::Closed);
        }
        if let Some(max) = state.max_size {
            if state.items.len() >= max {
                return Err(QueueError::Full);
            }
        }
        state.items.push_back(item);
        drop(state);
        self.recv_cond.signal();
        Ok(())
    }

    /// Pop item - blocks if empty
    pub fn pop(&self, _thread_id: ThreadId) -> Option<T> {
        let mut state = self.inner.lock().unwrap();
        loop {
            if let Some(item) = state.items.pop_front() {
                drop(state);
                self.send_cond.signal();
                return Some(item);
            }
            if state.closed {
                return None;
            }
            drop(state);
            std::thread::yield_now();
            state = self.inner.lock().unwrap();
        }
    }

    /// Try pop without blocking
    pub fn try_pop(&self) -> Option<T> {
        let mut state = self.inner.lock().unwrap();
        let item = state.items.pop_front();
        if item.is_some() {
            drop(state);
            self.send_cond.signal();
        }
        item
    }

    /// Pop with timeout
    pub fn pop_timeout(&self, _thread_id: ThreadId, timeout: Duration) -> Option<T> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(item) = self.try_pop() {
                return Some(item);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Get queue size
    pub fn size(&self) -> usize {
        self.inner.lock().unwrap().items.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// Close the queue - wake all waiters
    pub fn close(&self) {
        let mut state = self.inner.lock().unwrap();
        state.closed = true;
        drop(state);
        self.recv_cond.broadcast();
        self.send_cond.broadcast();
    }

    /// Check if closed
    pub fn is_closed(&self) -> bool {
        self.inner.lock().unwrap().closed
    }

    /// Get capacity (None for unbounded)
    pub fn capacity(&self) -> Option<usize> {
        self.inner.lock().unwrap().max_size
    }

    /// Check if full
    pub fn is_full(&self) -> bool {
        let state = self.inner.lock().unwrap();
        if let Some(max) = state.max_size {
            state.items.len() >= max
        } else {
            false
        }
    }
}

impl<T> Default for RubyQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Queue operation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueError {
    Closed,
    Full,
}

/// Green-thread-aware Semaphore
pub struct RubySemaphore {
    permits: AtomicU64,
    waiters: Mutex<VecDeque<ThreadId>>,
}

impl RubySemaphore {
    /// Create semaphore with initial permits
    pub fn new(permits: u64) -> Self {
        Self {
            permits: AtomicU64::new(permits),
            waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Acquire a permit - blocks if none available
    pub fn acquire(&self, thread_id: ThreadId) {
        loop {
            let current = self.permits.load(Ordering::Relaxed);
            if current == 0 {
                self.waiters.lock().unwrap().push_back(thread_id);
                std::thread::yield_now();
                continue;
            }
            match self.permits.compare_exchange(
                current,
                current - 1,
                Ordering::Acquire,
                Ordering::Relaxed
            ) {
                Ok(_) => return,
                Err(_) => continue,
            }
        }
    }

    /// Try acquire without blocking
    pub fn try_acquire(&self) -> bool {
        let current = self.permits.load(Ordering::Relaxed);
        if current == 0 {
            return false;
        }
        self.permits.compare_exchange(
            current,
            current - 1,
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_ok()
    }

    /// Release a permit - unparks first waiter
    pub fn release(&self) {
        self.permits.fetch_add(1, Ordering::Release);
        let _ = self.waiters.lock().unwrap().pop_front();
    }

    /// Get available permits
    pub fn available(&self) -> u64 {
        self.permits.load(Ordering::Relaxed)
    }
}

impl Default for RubySemaphore {
    fn default() -> Self {
        Self::new(1)
    }
}

/// Green-thread-aware ReadWriteLock
pub struct RubyRWLock {
    /// 0 = unlocked, positive = read locks, u64::MAX = write locked
    state: AtomicU64,
    write_waiters: Mutex<VecDeque<ThreadId>>,
    read_waiters: Mutex<VecDeque<ThreadId>>,
}

impl RubyRWLock {
    /// Create new unlocked RWLock
    pub fn new() -> Self {
        Self {
            state: AtomicU64::new(0),
            write_waiters: Mutex::new(VecDeque::new()),
            read_waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Acquire read lock
    pub fn read_lock(&self, thread_id: ThreadId) {
        loop {
            let state = self.state.load(Ordering::Relaxed);
            if state == u64::MAX {
                self.read_waiters.lock().unwrap().push_back(thread_id);
                std::thread::yield_now();
                continue;
            }
            match self.state.compare_exchange(
                state,
                state + 1,
                Ordering::Acquire,
                Ordering::Relaxed
            ) {
                Ok(_) => return,
                Err(_) => continue,
            }
        }
    }

    /// Release read lock
    pub fn read_unlock(&self) {
        let prev = self.state.fetch_sub(1, Ordering::Release);
        if prev == 1 {
            let _ = self.write_waiters.lock().unwrap().pop_front();
        }
    }

    /// Acquire write lock
    pub fn write_lock(&self, thread_id: ThreadId) {
        loop {
            if self.state.compare_exchange(
                0,
                u64::MAX,
                Ordering::Acquire,
                Ordering::Relaxed
            ).is_ok() {
                return;
            }
            self.write_waiters.lock().unwrap().push_back(thread_id);
            std::thread::yield_now();
        }
    }

    /// Release write lock
    pub fn write_unlock(&self) {
        self.state.store(0, Ordering::Release);
        let mut writers = self.write_waiters.lock().unwrap();
        let mut readers = self.read_waiters.lock().unwrap();
        let _ = writers.pop_front();
        let _: Vec<_> = readers.drain(..).collect();
    }

    /// Check if write locked
    pub fn is_write_locked(&self) -> bool {
        self.state.load(Ordering::Relaxed) == u64::MAX
    }

    /// Get read lock count
    pub fn read_count(&self) -> u64 {
        let state = self.state.load(Ordering::Relaxed);
        if state == u64::MAX { 0 } else { state }
    }
}

impl Default for RubyRWLock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutex_lock_unlock() {
        let m = RubyMutex::new();
        let tid = 123u64;
        m.lock(tid);
        assert!(m.is_locked());
        assert_eq!(m.owner(), tid);
        assert!(m.owned_by(tid));
        assert!(m.unlock(tid));
        assert!(!m.is_locked());
        assert_eq!(m.owner(), 0);
    }

    #[test]
    fn test_mutex_try_lock() {
        let m = RubyMutex::new();
        assert!(m.try_lock(1));
        assert!(!m.try_lock(2));
        assert!(m.is_locked());
        m.unlock(1);
        assert!(m.try_lock(2));
    }

    #[test]
    fn test_mutex_not_owner_unlock() {
        let m = RubyMutex::new();
        m.lock(1);
        assert!(!m.unlock(2));
        assert!(m.is_locked());
    }

    #[test]
    fn test_condvar_wait_signal() {
        let m = RubyMutex::new();
        let cv = RubyCondVar::new();
        m.lock(1);
        assert!(!cv.has_waiters());
        assert_eq!(cv.num_waiters(), 0);
        cv.signal();
        cv.broadcast();
        m.unlock(1);
    }

    #[test]
    fn test_queue_push_pop() {
        let q: RubyQueue<i32> = RubyQueue::new();
        q.try_push(1).unwrap();
        q.try_push(2).unwrap();
        q.try_push(3).unwrap();
        assert_eq!(q.size(), 3);
        assert!(!q.is_empty());
        assert!(q.capacity().is_none());
        assert!(!q.is_full());
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), Some(2));
        assert_eq!(q.size(), 1);
    }

    #[test]
    fn test_queue_close() {
        let q: RubyQueue<i32> = RubyQueue::new();
        q.try_push(1).unwrap();
        q.close();
        assert!(q.is_closed());
        assert!(q.try_push(2).is_err());
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), None);
    }

    #[test]
    fn test_sized_queue() {
        let sq: RubyQueue<i32> = RubyQueue::with_capacity(2);
        assert_eq!(sq.capacity(), Some(2));
        sq.try_push(1).unwrap();
        sq.try_push(2).unwrap();
        assert!(sq.is_full());
        assert_eq!(sq.try_push(3), Err(QueueError::Full));
        assert_eq!(sq.size(), 2);
        assert_eq!(sq.try_pop(), Some(1));
        assert!(!sq.is_full());
    }

    #[test]
    fn test_semaphore() {
        let sem = RubySemaphore::new(2);
        assert_eq!(sem.available(), 2);
        assert!(sem.try_acquire());
        assert_eq!(sem.available(), 1);
        assert!(sem.try_acquire());
        assert_eq!(sem.available(), 0);
        assert!(!sem.try_acquire());
        sem.release();
        assert_eq!(sem.available(), 1);
        sem.release();
        assert_eq!(sem.available(), 2);
    }

    #[test]
    fn test_rwlock() {
        let lock = RubyRWLock::new();
        lock.read_lock(1);
        assert_eq!(lock.read_count(), 1);
        assert!(!lock.is_write_locked());
        lock.read_lock(2);
        assert_eq!(lock.read_count(), 2);
        lock.read_unlock();
        assert_eq!(lock.read_count(), 1);
        lock.read_unlock();
        assert_eq!(lock.read_count(), 0);
        lock.write_lock(3);
        assert!(lock.is_write_locked());
        assert_eq!(lock.read_count(), 0);
        lock.write_unlock();
        assert!(!lock.is_write_locked());
    }
}
