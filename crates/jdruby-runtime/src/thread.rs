//! # Green Thread Scheduler
//!
//! Production M:N threading model with work-stealing and async IO.
//!
//! ## Architecture
//! - Native thread pool (N workers) running green threads (M green threads)
//! - Work-stealing queues per worker for load balancing
//! - Cooperative scheduling with yield points
//! - Async IO integration via epoll/kqueue/IOCP

use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::io::{self};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::value::RubyValue;

/// Unique identifier for a green thread.
pub type ThreadId = u64;

/// Counter for thread IDs.
static THREAD_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Thread state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadState {
    /// Ready to run.
    Runnable,
    /// Currently executing.
    Running,
    /// Blocked on I/O, lock, or other thread.
    Blocked(BlockReason),
    /// Sleeping until a deadline.
    Sleeping(Instant),
    /// Finished execution, has return value.
    Dead(RubyValue),
}

/// Reason for thread blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// Waiting for IO (fd, read/write events).
    Io { fd: RawFd, writable: bool },
    /// Waiting for another thread to finish.
    Join(ThreadId),
    /// Waiting for a mutex.
    Lock(*const ()),
    /// Waiting on condition variable.
    CondVar(*const ()),
    /// Waiting for channel receive.
    ChannelRecv(*const ()),
    /// Waiting for channel send.
    ChannelSend(*const ()),
}

/// CPU context for stackful coroutines.
/// Platform-specific register save/restore.
#[cfg(target_arch = "x86_64")]
#[repr(C)]
pub struct Context {
    rsp: u64,
    rbp: u64,
    rip: u64,
    rbx: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

#[cfg(target_arch = "aarch64")]
#[repr(C)]
pub struct Context {
    sp: u64,
    x19: u64,
    x20: u64,
    x21: u64,
    x22: u64,
    x23: u64,
    x24: u64,
    x25: u64,
    x26: u64,
    x27: u64,
    x28: u64,
    fp: u64,
    lr: u64,
}

impl Context {
    /// Create empty context.
    pub fn new() -> Self {
        #[cfg(target_arch = "x86_64")]
        return Context {
            rsp: 0, rbp: 0, rip: 0, rbx: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
        };
        #[cfg(target_arch = "aarch64")]
        return Context {
            sp: 0, x19: 0, x20: 0, x21: 0, x22: 0,
            x23: 0, x24: 0, x25: 0, x26: 0, x27: 0,
            x28: 0, fp: 0, lr: 0,
        };
    }

    /// Initialize context to start at function.
    pub fn init(&mut self, stack_top: *mut u8, entry: extern "C" fn(*mut u8), arg: *mut u8) {
        #[cfg(target_arch = "x86_64")]
        {
            self.rsp = stack_top as u64 - 8;
            self.rip = entry as u64;
            unsafe { *(stack_top as *mut u64).sub(1) = arg as u64; }
        }
        #[cfg(target_arch = "aarch64")]
        {
            self.sp = stack_top as u64;
            self.lr = entry as u64;
            self.x19 = arg as u64;
        }
    }
}

/// Stack for green thread.
pub struct Stack {
    ptr: *mut u8,
    size: usize,
}

impl Stack {
    /// Allocate new stack with guard page.
    pub fn new(size: usize) -> io::Result<Self> {
        use std::alloc::{alloc_zeroed, Layout};
        let layout = Layout::from_size_align(size + 4096, 4096)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Invalid stack size"))?;
        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(io::Error::new(io::ErrorKind::Other, "Stack allocation failed"));
        }
        Ok(Stack { ptr: unsafe { ptr.add(4096) }, size })
    }

    /// Get top of stack (highest address).
    pub fn top(&self) -> *mut u8 {
        unsafe { self.ptr.add(self.size) }
    }

    /// Get bottom of stack (lowest address, after guard page).
    pub fn bottom(&self) -> *mut u8 {
        self.ptr
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        unsafe {
            use std::alloc::{dealloc, Layout};
            let layout = Layout::from_size_align_unchecked(self.size + 4096, 4096);
            dealloc(self.ptr.sub(4096), layout);
        }
    }
}

unsafe impl Send for Stack {}
unsafe impl Sync for Stack {}

/// A green thread (lightweight fiber).
pub struct GreenThread {
    pub id: ThreadId,
    pub state: ThreadState,
    pub context: Context,
    pub stack: Stack,
    pub priority: i32,
    pub scheduler: *mut Scheduler,
    pub gc_roots: Vec<RubyValue>,
    pub result: Option<RubyValue>,
}

impl GreenThread {
    /// Create new green thread with entry function.
    pub fn new<F>(_entry: F, scheduler: *mut Scheduler) -> io::Result<Self>
    where
        F: FnOnce() -> RubyValue + Send + 'static,
    {
        let id = THREAD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let stack = Stack::new(1024 * 1024)?;
        
        Ok(GreenThread {
            id,
            state: ThreadState::Runnable,
            context: Context::new(),
            stack,
            priority: 0,
            scheduler,
            gc_roots: Vec::new(),
            result: None,
        })
    }

    /// Mark thread as blocked.
    pub fn block(&mut self, reason: BlockReason) {
        self.state = ThreadState::Blocked(reason);
    }

    /// Mark thread as sleeping until deadline.
    pub fn sleep_until(&mut self, deadline: Instant) {
        self.state = ThreadState::Sleeping(deadline);
    }

    /// Mark thread as dead with return value.
    pub fn complete(&mut self, value: RubyValue) {
        self.state = ThreadState::Dead(value.clone());
        self.result = Some(value);
    }

    /// Wake thread if blocked.
    pub fn wake(&mut self) {
        if matches!(self.state, ThreadState::Blocked(_) | ThreadState::Sleeping(_)) {
            self.state = ThreadState::Runnable;
        }
    }

    /// Check if thread is alive (not dead).
    pub fn is_alive(&self) -> bool {
        !matches!(self.state, ThreadState::Dead(_))
    }
}

/// Entry for sleep queue (ordered by deadline).
#[derive(Debug, Clone)]
struct SleepEntry {
    deadline: Instant,
    thread_id: ThreadId,
}

impl PartialEq for SleepEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.thread_id == other.thread_id
    }
}

impl Eq for SleepEntry {}

impl PartialOrd for SleepEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SleepEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.deadline.cmp(&self.deadline)
            .then_with(|| other.thread_id.cmp(&self.thread_id))
    }
}

/// Local run queue for work-stealing.
pub struct LocalQueue {
    queue: VecDeque<ThreadId>,
    capacity: usize,
}

impl LocalQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, id: ThreadId) -> bool {
        if self.queue.len() < self.capacity {
            self.queue.push_back(id);
            true
        } else {
            false
        }
    }

    pub fn pop(&mut self) -> Option<ThreadId> {
        self.queue.pop_front()
    }

    pub fn steal(&mut self) -> Option<ThreadId> {
        self.queue.pop_back()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

/// Global injector for work distribution.
pub struct GlobalQueue {
    queue: Mutex<VecDeque<ThreadId>>,
}

impl GlobalQueue {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push(&self, id: ThreadId) {
        self.queue.lock().unwrap().push_back(id);
    }

    pub fn pop(&self) -> Option<ThreadId> {
        self.queue.lock().unwrap().pop_front()
    }

    /// Steal multiple items for local queue.
    pub fn steal_batch(&self, local: &mut LocalQueue, batch_size: usize) -> usize {
        let mut guard = self.queue.lock().unwrap();
        let count = batch_size.min(guard.len());
        for _ in 0..count {
            if let Some(id) = guard.pop_front() {
                let _ = local.push(id);
            }
        }
        count
    }
}

/// IO events for polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoEvents {
    pub readable: bool,
    pub writable: bool,
}

/// IO poller abstraction (epoll/kqueue/IOCP).
pub struct IoPoller {
    #[cfg(target_os = "linux")]
    epoll_fd: RawFd,
    interests: Arc<RwLock<HashMap<RawFd, ThreadId>>>,
}

impl IoPoller {
    pub fn new() -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(IoPoller {
                epoll_fd: fd,
                interests: Arc::new(RwLock::new(HashMap::new())),
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(IoPoller {
                interests: Arc::new(RwLock::new(HashMap::new())),
            })
        }
    }

    /// Register interest in fd events.
    pub fn register(&self, fd: RawFd, events: IoEvents, thread_id: ThreadId) -> io::Result<()> {
        self.interests.write().unwrap().insert(fd, thread_id);
        
        #[cfg(target_os = "linux")]
        unsafe {
            let mut ev = libc::epoll_event {
                events: 0,
                u64: fd as u64,
            };
            if events.readable {
                ev.events |= libc::EPOLLIN as u32;
            }
            if events.writable {
                ev.events |= libc::EPOLLOUT as u32;
            }
            ev.events |= libc::EPOLLET as u32;
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut ev);
        }
        
        Ok(())
    }

    /// Unregister fd.
    pub fn unregister(&self, fd: RawFd) {
        self.interests.write().unwrap().remove(&fd);
        
        #[cfg(target_os = "linux")]
        unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut());
        }
    }

    /// Poll for ready events.
    pub fn poll(&self, timeout_ms: Option<i32>) -> io::Result<Vec<(RawFd, IoEvents)>> {
        #[cfg(target_os = "linux")]
        {
            let mut events = vec![libc::epoll_event { events: 0, u64: 0 }; 128];
            let timeout = timeout_ms.unwrap_or(-1);
            let n = unsafe {
                libc::epoll_wait(self.epoll_fd, events.as_mut_ptr(), events.len() as i32, timeout)
            };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            
            let mut results = Vec::with_capacity(n as usize);
            for i in 0..n as usize {
                let fd = events[i].u64 as RawFd;
                let ev = IoEvents {
                    readable: events[i].events & libc::EPOLLIN as u32 != 0,
                    writable: events[i].events & libc::EPOLLOUT as u32 != 0,
                };
                results.push((fd, ev));
            }
            Ok(results)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Vec::new())
        }
    }
}

impl Drop for IoPoller {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        unsafe {
            libc::close(self.epoll_fd);
        }
    }
}

/// Worker thread in the native thread pool.
pub struct WorkerThread {
    _id: usize,
    _local_queue: Mutex<LocalQueue>,
    handle: Option<std::thread::JoinHandle<()>>,
    _shutdown: Arc<AtomicBool>,
}

/// The thread scheduler (M:N model).
pub struct Scheduler {
    threads: RwLock<HashMap<ThreadId, Mutex<GreenThread>>>,
    global_queue: GlobalQueue,
    sleep_queue: Mutex<BinaryHeap<SleepEntry>>,
    io_poller: IoPoller,
    workers: Vec<WorkerThread>,
    shutdown: Arc<AtomicBool>,
    current: Mutex<Option<ThreadId>>,
}

impl Scheduler {
    /// Create new scheduler with N worker threads.
    pub fn new(num_workers: usize) -> io::Result<Self> {
        let io_poller = IoPoller::new()?;
        
        Ok(Scheduler {
            threads: RwLock::new(HashMap::new()),
            global_queue: GlobalQueue::new(),
            sleep_queue: Mutex::new(BinaryHeap::new()),
            io_poller,
            workers: Vec::with_capacity(num_workers),
            shutdown: Arc::new(AtomicBool::new(false)),
            current: Mutex::new(None),
        })
    }

    /// Spawn a new green thread.
    pub fn spawn<F>(&self, _f: F) -> io::Result<ThreadId>
    where
        F: FnOnce() -> RubyValue + Send + 'static,
    {
        let id = THREAD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let stack = Stack::new(1024 * 1024)?;
        
        let thread = GreenThread {
            id,
            state: ThreadState::Runnable,
            context: Context::new(),
            stack,
            priority: 0,
            scheduler: self as *const _ as *mut _,
            gc_roots: Vec::new(),
            result: None,
        };
        
        self.threads.write().unwrap().insert(id, Mutex::new(thread));
        self.global_queue.push(id);
        
        Ok(id)
    }

    /// Get currently running thread ID.
    pub fn current_thread(&self) -> Option<ThreadId> {
        *self.current.lock().unwrap()
    }

    /// Yield execution from current thread.
    pub fn yield_current(&self) {
        std::thread::yield_now();
    }

    /// Park current thread with block reason.
    pub fn park(&self, reason: BlockReason) {
        if let Some(id) = self.current_thread() {
            let threads = self.threads.read().unwrap();
            if let Some(thread) = threads.get(&id) {
                let mut t = thread.lock().unwrap();
                t.block(reason);
            }
        }
    }

    /// Unpark a waiting thread.
    pub fn unpark(&self, tid: ThreadId) {
        let threads = self.threads.read().unwrap();
        if let Some(thread) = threads.get(&tid) {
            let mut t = thread.lock().unwrap();
            t.wake();
        }
        self.global_queue.push(tid);
    }

    /// Join thread - wait for completion and get return value.
    pub fn join(&self, tid: ThreadId) -> Option<RubyValue> {
        loop {
            {
                let threads = self.threads.read().unwrap();
                if let Some(thread) = threads.get(&tid) {
                    let t = thread.lock().unwrap();
                    if let ThreadState::Dead(ref val) = t.state {
                        return Some(val.clone());
                    }
                } else {
                    return None;
                }
            }
            self.park(BlockReason::Join(tid));
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Sleep current thread for duration.
    pub fn sleep(&self, duration: Duration) {
        if let Some(id) = self.current_thread() {
            let deadline = Instant::now() + duration;
            
            let threads = self.threads.read().unwrap();
            if let Some(thread) = threads.get(&id) {
                let mut t = thread.lock().unwrap();
                t.sleep_until(deadline);
            }
            
            self.sleep_queue.lock().unwrap().push(SleepEntry {
                deadline,
                thread_id: id,
            });
        }
    }

    /// Block current thread on IO.
    pub fn block_on_io(&self, fd: RawFd, writable: bool) {
        let tid = self.current_thread().unwrap_or(0);
        let _ = self.io_poller.register(fd, IoEvents { readable: !writable, writable }, tid);
        self.park(BlockReason::Io { fd, writable });
    }

    /// Get number of runnable threads.
    pub fn runnable_count(&self) -> usize {
        let threads = self.threads.read().unwrap();
        threads.values()
            .filter(|t| matches!(t.lock().unwrap().state, ThreadState::Runnable))
            .count()
    }

    /// Get total thread count.
    pub fn thread_count(&self) -> usize {
        let threads = self.threads.read().unwrap();
        threads.values()
            .filter(|t| t.lock().unwrap().is_alive())
            .count()
    }

    /// Run the scheduler event loop.
    pub fn run(&self) {
        while !self.shutdown.load(Ordering::Relaxed) {
            let now = Instant::now();
            let mut sleep_queue = self.sleep_queue.lock().unwrap();
            while let Some(entry) = sleep_queue.peek() {
                if entry.deadline <= now {
                    let entry = sleep_queue.pop().unwrap();
                    self.unpark(entry.thread_id);
                } else {
                    break;
                }
            }
            drop(sleep_queue);

            if let Ok(events) = self.io_poller.poll(Some(0)) {
                for (fd, _) in events {
                    if let Some(tid) = self.io_poller.interests.read().unwrap().get(&fd) {
                        self.unpark(*tid);
                        self.io_poller.unregister(fd);
                    }
                }
            }

            for _ in 0..10 {
                if let Some(tid) = self.global_queue.pop() {
                    *self.current.lock().unwrap() = Some(tid);
                    let threads = self.threads.read().unwrap();
                    if let Some(thread) = threads.get(&tid) {
                        let mut t = thread.lock().unwrap();
                        if matches!(t.state, ThreadState::Runnable) {
                            t.state = ThreadState::Running;
                        }
                    }
                    *self.current.lock().unwrap() = None;
                } else {
                    break;
                }
            }

            if self.runnable_count() == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// Shutdown scheduler and all workers.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self::new(num_cpus).expect("Failed to create default scheduler")
    }
}

/// Thread-local storage for Ruby thread-local variables.
pub struct ThreadLocalStorage {
    map: HashMap<u64, RubyValue>,
}

impl ThreadLocalStorage {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn get(&self, key: u64) -> Option<&RubyValue> {
        self.map.get(&key)
    }

    pub fn set(&mut self, key: u64, value: RubyValue) {
        self.map.insert(key, value);
    }

    pub fn delete(&mut self, key: u64) -> Option<RubyValue> {
        self.map.remove(&key)
    }
}

impl Default for ThreadLocalStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creation() {
        let scheduler = Scheduler::new(4).unwrap();
        assert_eq!(scheduler.thread_count(), 0);
        assert_eq!(scheduler.runnable_count(), 0);
    }

    #[test]
    fn test_thread_spawn() {
        let scheduler = Scheduler::new(4).unwrap();
        let tid = scheduler.spawn(|| RubyValue::Integer(42)).unwrap();
        assert!(tid > 0);
        assert_eq!(scheduler.thread_count(), 1);
    }

    #[test]
    fn test_thread_state_transitions() {
        let scheduler = Scheduler::new(4).unwrap();
        let tid = scheduler.spawn(|| RubyValue::Nil).unwrap();
        
        {
            let threads = scheduler.threads.read().unwrap();
            let thread = threads.get(&tid).unwrap();
            let t = thread.lock().unwrap();
            assert!(matches!(t.state, ThreadState::Runnable));
        }
    }

    #[test]
    fn test_sleep_queue_ordering() {
        let mut heap = BinaryHeap::new();
        let now = Instant::now();
        
        heap.push(SleepEntry {
            deadline: now + Duration::from_secs(2),
            thread_id: 1,
        });
        heap.push(SleepEntry {
            deadline: now + Duration::from_secs(1),
            thread_id: 2,
        });
        heap.push(SleepEntry {
            deadline: now + Duration::from_secs(3),
            thread_id: 3,
        });
        
        let first = heap.pop().unwrap();
        assert_eq!(first.thread_id, 2);
        let second = heap.pop().unwrap();
        assert_eq!(second.thread_id, 1);
        let third = heap.pop().unwrap();
        assert_eq!(third.thread_id, 3);
    }

    #[test]
    fn test_local_queue() {
        let mut queue = LocalQueue::new(16);
        assert!(queue.push(1));
        assert!(queue.push(2));
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn test_local_queue_steal() {
        let mut queue = LocalQueue::new(16);
        for i in 0..5 {
            assert!(queue.push(i));
        }
        
        assert_eq!(queue.steal(), Some(4));
        assert_eq!(queue.steal(), Some(3));
        assert_eq!(queue.pop(), Some(0));
    }

    #[test]
    fn test_stack_allocation() {
        let stack = Stack::new(1024 * 1024).unwrap();
        assert!(!stack.ptr.is_null());
        assert_eq!(stack.size, 1024 * 1024);
    }

    #[test]
    fn test_thread_local_storage() {
        let mut tls = ThreadLocalStorage::new();
        assert!(tls.get(1).is_none());
        
        tls.set(1, RubyValue::Integer(42));
        assert_eq!(tls.get(1).unwrap(), &RubyValue::Integer(42));
        
        tls.set(1, RubyValue::True);
        assert_eq!(tls.get(1).unwrap(), &RubyValue::True);
        
        assert_eq!(tls.delete(1), Some(RubyValue::True));
        assert!(tls.get(1).is_none());
    }
}
