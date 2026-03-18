//! # Green Threads
//!
//! Lightweight cooperative threads with async support.
//! Inspired by Go's goroutines and Ruby's Fibers.
//!
//! ## Design
//! - M:N threading model (many green threads on few OS threads)
//! - Work-stealing scheduler
//! - Cooperative yielding at safe points (method calls, loops)
//! - Async I/O integration

/// Unique identifier for a green thread.
pub type ThreadId = u64;

/// State of a green thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    /// Ready to be scheduled.
    Ready,
    /// Currently executing.
    Running,
    /// Waiting for I/O or another thread.
    Blocked,
    /// Finished execution.
    Done,
}

/// A green thread (lightweight fiber).
#[derive(Debug)]
pub struct GreenThread {
    pub id: ThreadId,
    pub state: ThreadState,
    // TODO: stack pointer, instruction pointer, local variables
}

/// The thread scheduler.
pub struct Scheduler {
    threads: Vec<GreenThread>,
    next_id: ThreadId,
    current: Option<ThreadId>,
}

impl Scheduler {
    /// Create a new scheduler.
    pub fn new() -> Self {
        Self {
            threads: Vec::new(),
            next_id: 1,
            current: None,
        }
    }

    /// Spawn a new green thread.
    pub fn spawn(&mut self) -> ThreadId {
        let id = self.next_id;
        self.next_id += 1;
        self.threads.push(GreenThread {
            id,
            state: ThreadState::Ready,
        });
        id
    }

    /// Get the currently running thread ID.
    pub fn current_thread(&self) -> Option<ThreadId> {
        self.current
    }

    /// Yield execution from the current thread.
    pub fn yield_current(&mut self) {
        // TODO: Save current thread state, pick next ready thread
    }

    /// Get the number of active threads.
    pub fn thread_count(&self) -> usize {
        self.threads.iter().filter(|t| t.state != ThreadState::Done).count()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
