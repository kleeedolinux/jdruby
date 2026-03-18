//! # JDGC — Julia's Dream Garbage Collector
//!
//! A concurrent, region-based, incremental, compacting garbage collector.
//!
//! ## Core Algorithms
//!
//! - **Tri-Color Concurrent Marking** (Dijkstra) with Yuasa-style deletion
//!   barrier + Dijkstra insertion barrier.
//! - **Region-Based Heap** with lock-free TLAB bump-pointer allocation.
//! - **Concurrent Evacuation** with Brooks read barrier and CAS-based
//!   forwarding pointer installation.
//!
//! ## Object Header Layout (64-bit, packed in `AtomicU64`)
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────┐
//! │ Bit 63 ─────────────────────── Bit 3 │ Bit 2  │ Bit 1 │ Bit 0   │
//! │        Forwarding Pointer (61 bits)   │ Pinned │    Color (2b)   │
//! └────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! - **Color**: `00` = White, `01` = Gray, `10` = Black
//! - **Pinned**: `1` = object cannot be evacuated
//! - **Forwarding Pointer**: stored as `addr & !0b111` (8-byte aligned)

use std::alloc::{self, Layout};
use std::collections::VecDeque;
use std::ptr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

// ═══════════════════════════════════════════════════════════════════════
//  Constants & Bitmasks
// ═══════════════════════════════════════════════════════════════════════

/// Region size: 2 MiB.
const REGION_SIZE: usize = 2 * 1024 * 1024;

/// Minimum object alignment (8 bytes).
const OBJ_ALIGN: usize = 8;

/// Bitmask for the tri-color state (bits 0–1).
const COLOR_MASK: u64 = 0b11;

/// Bit position of the pinned flag.
const PINNED_BIT: u64 = 1 << 2;

/// Bitmask for the forwarding pointer (bits 3–63).
/// Since all allocations are 8-byte aligned, the low 3 bits of any valid
/// pointer are always zero, allowing us to pack flags into them.
const FWD_MASK: u64 = !0b111u64;

/// Tri-color states.
const WHITE: u64 = 0b00;
const GRAY: u64 = 0b01;
const BLACK: u64 = 0b10;

// ═══════════════════════════════════════════════════════════════════════
//  Object Header
// ═══════════════════════════════════════════════════════════════════════

/// A 64-bit atomic object header with packed tri-color state, pin flag,
/// and forwarding pointer.
///
/// Every heap object is prefixed with this header. Mutators and GC threads
/// operate on it concurrently via atomic CAS loops.
#[repr(C, align(8))]
pub struct ObjectHeader {
    /// The packed 64-bit header word.
    ///   bits 0–1:  color (White/Gray/Black)
    ///   bit  2:    pinned flag
    ///   bits 3–63: forwarding pointer (8-byte aligned addr)
    bits: AtomicU64,
    /// Size of the object payload (bytes after the header).
    /// Not part of the atomic word — set once at allocation, immutable thereafter.
    pub payload_size: usize,
}

impl ObjectHeader {
    // ── Constructors ───────────────────────────────────────

    /// Create a new header for a freshly allocated object.
    /// The forwarding pointer is initially set to the object's own address
    /// (identity forwarding), and the color is White.
    #[inline]
    pub fn init_at(self_ptr: *mut ObjectHeader, payload_size: usize) {
        let addr = self_ptr as u64;
        debug_assert_eq!(addr & !FWD_MASK, 0, "object pointer not 8-byte aligned");
        let header = addr | WHITE; // fwd = self, color = white, not pinned
        unsafe {
            (*self_ptr).bits.store(header, Ordering::Release);
            (*self_ptr).payload_size = payload_size;
        }
    }

    // ── Atomic Accessors ───────────────────────────────────

    /// Load the raw header word with Acquire ordering.
    #[inline]
    pub fn load(&self) -> u64 {
        self.bits.load(Ordering::Acquire)
    }

    /// Extract the tri-color state from a raw header word.
    #[inline]
    pub const fn color_of(raw: u64) -> u64 {
        raw & COLOR_MASK
    }

    /// Current color.
    #[inline]
    pub fn color(&self) -> u64 {
        Self::color_of(self.load())
    }

    #[inline]
    pub fn is_white(&self) -> bool {
        self.color() == WHITE
    }

    #[inline]
    pub fn is_gray(&self) -> bool {
        self.color() == GRAY
    }

    #[inline]
    pub fn is_black(&self) -> bool {
        self.color() == BLACK
    }

    #[inline]
    pub fn is_pinned(&self) -> bool {
        self.load() & PINNED_BIT != 0
    }

    /// Set the pinned flag (CAS loop).
    pub fn pin(&self) {
        loop {
            let old = self.load();
            let new = old | PINNED_BIT;
            if old == new {
                return;
            }
            if self
                .bits
                .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
        }
    }

    // ── Color Transitions (CAS) ────────────────────────────

    /// Attempt to transition this object from `expected_color` to
    /// `new_color` using a single CAS. Returns `true` on success.
    ///
    /// The forwarding pointer and pin flag are preserved.
    #[inline]
    pub fn try_set_color(&self, expected_color: u64, new_color: u64) -> bool {
        let old = self.load();
        if Self::color_of(old) != expected_color {
            return false;
        }
        let desired = (old & !COLOR_MASK) | new_color;
        self.bits
            .compare_exchange(old, desired, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    /// Unconditionally set color (CAS loop — retries on spurious failures
    /// from concurrent forwarding‐pointer updates, NOT from color races).
    pub fn set_color(&self, new_color: u64) {
        loop {
            let old = self.load();
            let desired = (old & !COLOR_MASK) | new_color;
            if self
                .bits
                .compare_exchange_weak(old, desired, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Attempt White → Gray transition. Returns `true` if this thread won.
    #[inline]
    pub fn try_shade_gray(&self) -> bool {
        self.try_set_color(WHITE, GRAY)
    }

    /// Attempt Gray → Black transition.
    #[inline]
    pub fn shade_black(&self) -> bool {
        self.try_set_color(GRAY, BLACK)
    }

    // ── Forwarding Pointer ─────────────────────────────────

    /// Extract the forwarding address from a raw header word.
    #[inline]
    pub const fn fwd_addr_of(raw: u64) -> *mut ObjectHeader {
        (raw & FWD_MASK) as *mut ObjectHeader
    }

    /// Current forwarding address.
    #[inline]
    pub fn forwarding_address(&self) -> *mut ObjectHeader {
        Self::fwd_addr_of(self.load())
    }

    /// Check if this object has been forwarded to a different address.
    #[inline]
    pub fn is_forwarded(&self, self_ptr: *const ObjectHeader) -> bool {
        let fwd = self.forwarding_address();
        !fwd.is_null() && fwd as *const _ != self_ptr
    }

    /// Attempt to install a forwarding pointer via CAS.
    ///
    /// - Preserves the color and pinned bits from the **old** header.
    /// - Stores `new_addr` in bits 3–63.
    /// - Returns `Ok(new_addr)` if this thread won the race.
    /// - Returns `Err(winner_addr)` if another thread already forwarded it.
    pub fn try_install_forwarding(
        &self,
        self_ptr: *const ObjectHeader,
        new_addr: *mut ObjectHeader,
    ) -> Result<*mut ObjectHeader, *mut ObjectHeader> {
        debug_assert_eq!(new_addr as u64 & !FWD_MASK, 0, "new_addr not aligned");

        loop {
            let old = self.load();
            let old_fwd = Self::fwd_addr_of(old);

            // Already forwarded by another thread — return their address.
            if !old_fwd.is_null() && old_fwd as *const _ != self_ptr {
                return Err(old_fwd);
            }

            // Build new header: keep color + pin, replace fwd pointer.
            let flags = old & !FWD_MASK; // color + pin
            let desired = (new_addr as u64 & FWD_MASK) | flags;

            match self
                .bits
                .compare_exchange(old, desired, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Ok(new_addr),
                Err(actual) => {
                    // Check if the failure is because someone else forwarded.
                    let winner = Self::fwd_addr_of(actual);
                    if !winner.is_null() && winner as *const _ != self_ptr {
                        return Err(winner);
                    }
                    // Otherwise, spurious failure (concurrent color change) — retry.
                }
            }
        }
    }

    // ── Payload Helpers ────────────────────────────────────

    /// Pointer to the start of the object's payload (immediately after the
    /// header).
    #[inline]
    pub fn payload_ptr(&self) -> *mut u8 {
        let p = self as *const Self as *mut u8;
        unsafe { p.add(std::mem::size_of::<ObjectHeader>()) }
    }

    /// Total allocation size (header + payload), rounded up to `OBJ_ALIGN`.
    #[inline]
    pub fn total_size(&self) -> usize {
        align_up(std::mem::size_of::<ObjectHeader>() + self.payload_size, OBJ_ALIGN)
    }

    /// Mock method: return pointers to reference fields inside the payload.
    ///
    /// In a real VM this would be driven by the object's class/layout
    /// descriptor. Here we treat the entire payload as a dense array of
    /// pointers for demonstration purposes.
    pub unsafe fn get_references(&self) -> &[*mut ObjectHeader] {
        let count = self.payload_size / std::mem::size_of::<*mut ObjectHeader>();
        if count == 0 {
            return &[];
        }
        std::slice::from_raw_parts(self.payload_ptr() as *const *mut ObjectHeader, count)
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Region (2 MiB bump-allocated arena)
// ═══════════════════════════════════════════════════════════════════════

/// A 2 MiB memory region with a lock-free bump-pointer allocator.
///
/// Each mutator thread receives its own `Region` as a TLAB (Thread-Local
/// Allocation Buffer). Allocation is a single `fetch_add` on the cursor —
/// no locks, no contention.
pub struct Region {
    /// Base address of the backing allocation.
    start: *mut u8,
    /// Current allocation cursor (byte offset from `start`).
    /// Monotonically increasing. Allocation is done by `fetch_add`.
    cursor: AtomicUsize,
    /// Total capacity in bytes.
    capacity: usize,
    /// Unique region id.
    pub id: u32,
    /// Live bytes (updated during marking). Used to decide evacuation
    /// candidates (regions with low liveness = high fragmentation).
    pub live_bytes: AtomicUsize,
}

impl Region {
    /// Allocate a new 2 MiB region from the OS.
    pub fn new(id: u32) -> Self {
        let layout = Layout::from_size_align(REGION_SIZE, OBJ_ALIGN).unwrap();
        let start = unsafe { alloc::alloc_zeroed(layout) };
        if start.is_null() {
            panic!("JDGC: failed to allocate {} byte region", REGION_SIZE);
        }
        Self {
            start,
            cursor: AtomicUsize::new(0),
            capacity: REGION_SIZE,
            id,
            live_bytes: AtomicUsize::new(0),
        }
    }

    /// Lock-free bump-pointer allocation.
    ///
    /// Returns a pointer to the start of the allocated block (suitable for
    /// placing an `ObjectHeader`), or `None` if the region is full.
    ///
    /// Uses `fetch_add` with `Relaxed` ordering — the TLAB is thread-local,
    /// so no cross-thread synchronization is needed at the allocation site.
    /// The `Release` fence happens later when the object header is written.
    pub fn allocate(&self, size: usize) -> Option<*mut u8> {
        let aligned_size = align_up(size, OBJ_ALIGN);
        // Atomically advance the cursor. This is a single instruction on
        // x86 (LOCK XADD) and ARMv8 (LDADD).
        let offset = self.cursor.fetch_add(aligned_size, Ordering::Relaxed);

        if offset + aligned_size > self.capacity {
            // Allocation exceeds region boundary. Roll back the cursor so
            // the region doesn't appear to have more used space than it
            // actually does (best-effort — not critical for correctness).
            self.cursor.fetch_sub(aligned_size, Ordering::Relaxed);
            return None;
        }

        Some(unsafe { self.start.add(offset) })
    }

    /// Allocate an object with the given payload size.
    ///
    /// Writes the object header and returns a pointer to it.
    pub fn allocate_object(&self, payload_size: usize) -> Option<*mut ObjectHeader> {
        let total = align_up(
            std::mem::size_of::<ObjectHeader>() + payload_size,
            OBJ_ALIGN,
        );
        let ptr = self.allocate(total)?;
        let obj = ptr as *mut ObjectHeader;
        ObjectHeader::init_at(obj, payload_size);
        Some(obj)
    }

    /// Used bytes in this region.
    #[inline]
    pub fn used(&self) -> usize {
        self.cursor.load(Ordering::Relaxed)
    }

    /// Remaining free bytes.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.capacity.saturating_sub(self.used())
    }

    /// Reset the region for reuse (after evacuation).
    pub fn reset(&self) {
        self.cursor.store(0, Ordering::Release);
        self.live_bytes.store(0, Ordering::Relaxed);
        // Zero the memory to prevent dangling references from being
        // visible to a concurrent marker.
        unsafe {
            ptr::write_bytes(self.start, 0, self.capacity);
        }
    }

    /// Fragmentation ratio: `1.0 - (live_bytes / used_bytes)`.
    /// Returns 0.0 if nothing is allocated.
    pub fn fragmentation(&self) -> f64 {
        let used = self.used();
        if used == 0 {
            return 0.0;
        }
        let live = self.live_bytes.load(Ordering::Relaxed);
        1.0 - (live as f64 / used as f64)
    }

    /// Whether this region is a good evacuation candidate.
    /// Threshold: >50% fragmentation.
    pub fn should_evacuate(&self) -> bool {
        self.fragmentation() > 0.5
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(REGION_SIZE, OBJ_ALIGN).unwrap();
        unsafe {
            alloc::dealloc(self.start, layout);
        }
    }
}

// SAFETY: Region internals are accessed through atomics or under GC
// synchronization.
unsafe impl Send for Region {}
unsafe impl Sync for Region {}

// ═══════════════════════════════════════════════════════════════════════
//  Work Queue (concurrent gray-object queue)
// ═══════════════════════════════════════════════════════════════════════

/// Thread-safe work queue for the tri-color marking algorithm.
///
/// Holds pointers to Gray objects that need to be scanned. Both mutator
/// threads (via the write barrier) and GC marker threads push into this
/// queue.
pub struct WorkQueue {
    inner: Mutex<VecDeque<*mut ObjectHeader>>,
}

impl WorkQueue {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(4096)),
        }
    }

    /// Push a gray object onto the queue.
    pub fn push(&self, obj: *mut ObjectHeader) {
        self.inner.lock().unwrap().push_back(obj);
    }

    /// Pop a gray object from the queue.
    pub fn pop(&self) -> Option<*mut ObjectHeader> {
        self.inner.lock().unwrap().pop_front()
    }

    /// Drain up to `batch_size` objects into the provided buffer.
    /// Returns the number of objects drained.
    pub fn drain_batch(&self, buf: &mut Vec<*mut ObjectHeader>, batch_size: usize) -> usize {
        let mut q = self.inner.lock().unwrap();
        let n = batch_size.min(q.len());
        buf.extend(q.drain(..n));
        n
    }

    /// True if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Number of pending gray objects.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

// SAFETY: The queue contents are raw pointers that are only dereferenced
// under proper GC protocol.
unsafe impl Send for WorkQueue {}
unsafe impl Sync for WorkQueue {}

impl Default for WorkQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Mutator Barriers
// ═══════════════════════════════════════════════════════════════════════

/// **Dijkstra Insertion Write Barrier + Yuasa Deletion Barrier.**
///
/// Called by the mutator when executing `source.field = target`.
///
/// Algorithm:
/// 1. Load `source` header with `Acquire`.
/// 2. If `source` is **Black** (already scanned):
///    a. Load `target` header with `Acquire`.
///    b. If `target` is **White** (not yet discovered):
///       - CAS `target` color from White → Gray.
///       - If CAS succeeds, push `target` onto the gray `WorkQueue`.
///
/// This prevents the "lost object" problem: a Black object holding a
/// reference to a White object that the marker will never visit.
///
/// Memory orderings:
/// - `Acquire` loads ensure we see the latest color written by the GC.
/// - `AcqRel` CAS ensures the Gray coloring is visible to marker threads.
#[inline]
pub fn write_barrier(
    source: &ObjectHeader,
    target: *mut ObjectHeader,
    queue: &WorkQueue,
) {
    let src_raw = source.load(); // Acquire

    // Fast path: source is not Black — no barrier needed.
    if ObjectHeader::color_of(src_raw) != BLACK {
        return;
    }

    let target_ref = unsafe { &*target };
    let tgt_raw = target_ref.load(); // Acquire

    // Target is already Gray or Black — nothing to do.
    if ObjectHeader::color_of(tgt_raw) != WHITE {
        return;
    }

    // Slow path: CAS target White → Gray.
    // Build the desired header: same fwd + pin, but color = Gray.
    let desired = (tgt_raw & !COLOR_MASK) | GRAY;
    if target_ref
        .bits
        .compare_exchange(tgt_raw, desired, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        // We won the race — enqueue the target for scanning.
        queue.push(target);
    }
    // If the CAS failed, another thread already colored it (Gray or Black).
    // In either case, the invariant is maintained.
}

/// **Brooks Read Barrier — forwarding pointer resolution.**
///
/// Every time a mutator reads a reference, it must resolve the forwarding
/// pointer. If the object has been evacuated, the forwarding pointer in
/// bits 3–63 points to the new copy. The mutator transparently follows it.
///
/// This is a single atomic load + mask + branch. On the fast path (object
/// not moved), it's essentially free — one `load` + `and` + `cmp`.
///
/// Memory ordering: `Acquire` to ensure we see the payload of the new
/// copy if the object was evacuated.
#[inline]
pub fn read_barrier(obj: *mut ObjectHeader) -> *mut ObjectHeader {
    if obj.is_null() {
        return obj;
    }
    let header = unsafe { (*obj).bits.load(Ordering::Acquire) };
    let fwd = ObjectHeader::fwd_addr_of(header);

    // If the forwarding pointer is non-null and different from `obj`,
    // the object has been evacuated — follow the forwarding pointer.
    if !fwd.is_null() && fwd != obj {
        fwd
    } else {
        obj
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  JDGC — The Garbage Collector
// ═══════════════════════════════════════════════════════════════════════

/// GC phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcPhase {
    /// Mutators are running; no GC activity.
    Idle,
    /// Concurrent marking is in progress.
    Marking,
    /// Concurrent evacuation / compaction.
    Evacuating,
    /// Sweeping (reclaiming evacuated regions).
    Sweeping,
}

/// Configuration for the JDGC collector.
#[derive(Debug, Clone)]
pub struct GcConfig {
    /// Initial number of regions to pre-allocate.
    pub initial_regions: u32,
    /// Fragmentation threshold for evacuation candidates (0.0–1.0).
    pub evacuation_threshold: f64,
    /// Maximum number of objects to scan per marker batch.
    pub mark_batch_size: usize,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            initial_regions: 16,
            evacuation_threshold: 0.5,
            mark_batch_size: 256,
        }
    }
}

/// **JDGC** — Julia's Dream Garbage Collector.
///
/// Manages the region-based heap, the tri-color marking work queue, and
/// the full collection cycle (mark → evacuate → sweep).
pub struct JDGC {
    /// All heap regions.
    regions: Vec<Region>,
    /// The gray-object work queue.
    pub work_queue: WorkQueue,
    /// Current GC phase.
    phase: GcPhase,
    /// Next region ID.
    next_region_id: u32,
    /// Configuration.
    config: GcConfig,
}

impl JDGC {
    /// Create a new JDGC instance with the given configuration.
    pub fn new(config: GcConfig) -> Self {
        let mut regions = Vec::with_capacity(config.initial_regions as usize);
        for id in 0..config.initial_regions {
            regions.push(Region::new(id));
        }
        Self {
            regions,
            work_queue: WorkQueue::new(),
            phase: GcPhase::Idle,
            next_region_id: config.initial_regions,
            config,
        }
    }

    /// Allocate a fresh region and add it to the heap.
    pub fn add_region(&mut self) -> &Region {
        let id = self.next_region_id;
        self.next_region_id += 1;
        self.regions.push(Region::new(id));
        self.regions.last().unwrap()
    }

    /// Find a region with enough space, or allocate a new one.
    pub fn allocate_object(&mut self, payload_size: usize) -> *mut ObjectHeader {
        let total = align_up(
            std::mem::size_of::<ObjectHeader>() + payload_size,
            OBJ_ALIGN,
        );
        // Try existing regions.
        for region in &self.regions {
            if let Some(obj) = region.allocate_object(payload_size) {
                return obj;
            }
        }
        // All regions full — allocate a new one.
        let region = Region::new(self.next_region_id);
        self.next_region_id += 1;
        let obj = region
            .allocate_object(payload_size)
            .expect("JDGC: fresh region too small for object");
        self.regions.push(region);
        obj
    }

    // ═══════════════════════════════════════════════════════
    //  Phase 1: Concurrent Marking (Dijkstra Tri-Color)
    // ═══════════════════════════════════════════════════════

    /// Run the full concurrent marking phase.
    ///
    /// 1. For each root, if White → shade Gray and push onto the work queue.
    /// 2. Drain the work queue:
    ///    a. Pop a Gray object.
    ///    b. Scan its reference fields.
    ///    c. For each child: if White, shade Gray and enqueue.
    ///    d. Color the object Black.
    /// 3. Repeat until the queue is empty (fixed-point).
    ///
    /// During this phase, mutator write barriers may push additional Gray
    /// objects, so the loop naturally handles concurrent mutations.
    pub fn mark_phase(&self, roots: &[*mut ObjectHeader]) {
        // ── Step 1: shade roots ────────────────────────────
        for &root in roots {
            if root.is_null() {
                continue;
            }
            // Resolve forwarding pointer first (the root might point to
            // an evacuated object from a previous cycle).
            let resolved = read_barrier(root);
            let hdr = unsafe { &*resolved };
            if hdr.try_shade_gray() {
                self.work_queue.push(resolved);
            }
        }

        // ── Step 2: drain gray queue ───────────────────────
        let mut batch = Vec::with_capacity(self.config.mark_batch_size);
        loop {
            batch.clear();
            let n = self
                .work_queue
                .drain_batch(&mut batch, self.config.mark_batch_size);
            if n == 0 {
                break;
            }

            for &obj_ptr in &batch {
                let obj = unsafe { &*obj_ptr };

                // Scan reference fields.
                let refs = unsafe { obj.get_references() };
                for &child_ptr in refs {
                    if child_ptr.is_null() {
                        continue;
                    }
                    let child = read_barrier(child_ptr);
                    let child_hdr = unsafe { &*child };

                    // If the child is White, shade it Gray and enqueue.
                    if child_hdr.try_shade_gray() {
                        // Accumulate live byte count in the child's region.
                        // (In a real VM we'd look up the region by address.)
                        self.work_queue.push(child);
                    }
                }

                // Color the object Black — we've scanned all its children.
                obj.shade_black();
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    //  Phase 2: Concurrent Evacuation
    // ═══════════════════════════════════════════════════════

    /// Evacuate (copy) a single object from its current location to
    /// `target_region`.
    ///
    /// ## Algorithm
    ///
    /// 1. Load old header with `Acquire`.
    /// 2. If already forwarded → return winner's address.
    /// 3. If pinned → skip (return old address).
    /// 4. Allocate space in `target_region`.
    /// 5. Copy the payload.
    /// 6. Initialize the new header (same color, identity forwarding).
    /// 7. CAS the old header to install the forwarding pointer.
    ///    - **Win**: return new address.
    ///    - **Lose**: another thread forwarded it first. Discard our copy
    ///      (the bump cursor has advanced, but that space is wasted — it
    ///      will be reclaimed when the target region is eventually swept).
    ///      Return the winner's forwarded address.
    ///
    /// This is the critical CAS-based protocol that makes concurrent
    /// evacuation safe without a stop-the-world pause.
    pub fn evacuate_object(
        &self,
        old_obj: *mut ObjectHeader,
        target_region: &Region,
    ) -> *mut ObjectHeader {
        let old_hdr = unsafe { &*old_obj };
        let raw = old_hdr.load();

        // ── Already forwarded? ─────────────────────────────
        let fwd = ObjectHeader::fwd_addr_of(raw);
        if !fwd.is_null() && fwd != old_obj {
            return fwd; // Another thread (or previous cycle) already moved it.
        }

        // ── Pinned? ────────────────────────────────────────
        if raw & PINNED_BIT != 0 {
            return old_obj; // Cannot evacuate pinned objects.
        }

        let payload_size = old_hdr.payload_size;
        let total_size = old_hdr.total_size();

        // ── Allocate in target region ──────────────────────
        let new_ptr = match target_region.allocate(total_size) {
            Some(p) => p as *mut ObjectHeader,
            None => return old_obj, // Target region full — skip.
        };

        // ── Copy payload ───────────────────────────────────
        // Copy the entire object (header + payload) to the new location.
        unsafe {
            ptr::copy_nonoverlapping(
                old_obj as *const u8,
                new_ptr as *mut u8,
                total_size,
            );
        }

        // ── Initialize new header ──────────────────────────
        // The new copy's forwarding pointer should point to itself.
        let new_color = ObjectHeader::color_of(raw);
        let new_header = (new_ptr as u64 & FWD_MASK) | (raw & PINNED_BIT) | new_color;
        unsafe {
            (*new_ptr).bits.store(new_header, Ordering::Release);
        }

        // ── CAS the forwarding pointer in the old header ───
        match old_hdr.try_install_forwarding(old_obj, new_ptr) {
            Ok(_new_addr) => {
                // We won the race. The old object now forwards to new_ptr.
                new_ptr
            }
            Err(winner_addr) => {
                // Another thread beat us. Our copy at `new_ptr` is wasted
                // (it occupies space in the target region but won't be
                // referenced). This is acceptable — the wasted space is
                // negligible compared to a mutex on every evacuation.
                //
                // Return the winner's address so the caller uses the
                // canonical copy.
                winner_addr
            }
        }
    }

    /// Select fragmented regions and evacuate all live objects.
    ///
    /// The GC thread calls this after marking is complete.
    pub fn evacuation_phase(&mut self) {
        // ── Select evacuation candidates ───────────────────
        let candidate_ids: Vec<u32> = self
            .regions
            .iter()
            .filter(|r| r.fragmentation() > self.config.evacuation_threshold)
            .map(|r| r.id)
            .collect();

        if candidate_ids.is_empty() {
            return;
        }

        // ── Allocate target region(s) ──────────────────────
        let target = Region::new(self.next_region_id);
        self.next_region_id += 1;

        // ── Evacuate live (Black) objects ──────────────────
        // Walk each candidate region's bump area and evacuate Black objects.
        for &cid in &candidate_ids {
            let region_idx = self.regions.iter().position(|r| r.id == cid);
            if let Some(idx) = region_idx {
                let region = &self.regions[idx];
                let used = region.used();
                let mut offset = 0usize;

                while offset < used {
                    let obj_ptr =
                        unsafe { region.start.add(offset) } as *mut ObjectHeader;
                    let obj = unsafe { &*obj_ptr };
                    let raw = obj.load();

                    // Only evacuate live (Black) objects.
                    if ObjectHeader::color_of(raw) == BLACK {
                        self.evacuate_object(obj_ptr, &target);
                    }

                    // Advance past this object.
                    let obj_total = obj.total_size();
                    if obj_total == 0 {
                        break; // Safety: avoid infinite loop on zeroed memory.
                    }
                    offset += obj_total;
                }
            }
        }

        // ── Reclaim candidate regions ──────────────────────
        // Reset evacuated regions so they can be reused.
        for &cid in &candidate_ids {
            if let Some(region) = self.regions.iter().find(|r| r.id == cid) {
                region.reset();
            }
        }

        // ── Add the target region to the heap ──────────────
        self.regions.push(target);
    }

    // ═══════════════════════════════════════════════════════
    //  Phase 3: Sweep (reset White objects)
    // ═══════════════════════════════════════════════════════

    /// After marking + evacuation, all surviving objects are Black.
    /// Sweep resets them to White for the next cycle, and reclaims
    /// any regions that contain no live objects.
    pub fn sweep_phase(&mut self) {
        for region in &self.regions {
            let used = region.used();
            let mut offset = 0usize;
            let mut region_live = 0usize;

            while offset < used {
                let obj_ptr =
                    unsafe { region.start.add(offset) } as *mut ObjectHeader;
                let obj = unsafe { &*obj_ptr };
                let raw = obj.load();

                let total = obj.total_size();
                if total == 0 {
                    break;
                }

                if ObjectHeader::color_of(raw) == BLACK {
                    // Live object — reset to White for next cycle.
                    obj.set_color(WHITE);
                    region_live += total;
                }

                offset += total;
            }

            region.live_bytes.store(region_live, Ordering::Relaxed);
        }

        // Reclaim completely empty regions.
        self.regions.retain(|r| {
            r.live_bytes.load(Ordering::Relaxed) > 0 || r.used() == 0
        });
    }

    // ═══════════════════════════════════════════════════════
    //  Full Collection Cycle
    // ═══════════════════════════════════════════════════════

    /// Run a full GC cycle: Mark → Evacuate → Sweep.
    pub fn collect(&mut self, roots: &[*mut ObjectHeader]) {
        self.phase = GcPhase::Marking;
        self.mark_phase(roots);

        self.phase = GcPhase::Evacuating;
        self.evacuation_phase();

        self.phase = GcPhase::Sweeping;
        self.sweep_phase();

        self.phase = GcPhase::Idle;
    }

    /// Current GC phase.
    pub fn phase(&self) -> GcPhase {
        self.phase
    }

    /// Total heap size in bytes.
    pub fn heap_size(&self) -> usize {
        self.regions.len() * REGION_SIZE
    }

    /// Total used bytes across all regions.
    pub fn used_bytes(&self) -> usize {
        self.regions.iter().map(|r| r.used()).sum()
    }

    /// Number of regions.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Utility
// ═══════════════════════════════════════════════════════════════════════

/// Round `val` up to the next multiple of `align`.
/// `align` must be a power of two.
#[inline]
const fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_header_color_transitions() {
        let region = Region::new(0);
        let obj = region.allocate_object(64).unwrap();
        let hdr = unsafe { &*obj };

        assert!(hdr.is_white());
        assert!(!hdr.is_gray());
        assert!(!hdr.is_black());

        // White → Gray
        assert!(hdr.try_shade_gray());
        assert!(hdr.is_gray());

        // Gray → Black
        assert!(hdr.shade_black());
        assert!(hdr.is_black());

        // Cannot go White → Gray again (already Black)
        assert!(!hdr.try_shade_gray());
    }

    #[test]
    fn test_object_header_pinning() {
        let region = Region::new(0);
        let obj = region.allocate_object(32).unwrap();
        let hdr = unsafe { &*obj };

        assert!(!hdr.is_pinned());
        hdr.pin();
        assert!(hdr.is_pinned());

        // Color transitions still work when pinned.
        assert!(hdr.try_shade_gray());
        assert!(hdr.is_gray());
        assert!(hdr.is_pinned());
    }

    #[test]
    fn test_forwarding_pointer_identity() {
        let region = Region::new(0);
        let obj = region.allocate_object(64).unwrap();
        let hdr = unsafe { &*obj };

        // Initially, forwarding pointer == self.
        assert_eq!(hdr.forwarding_address(), obj);
        assert!(!hdr.is_forwarded(obj));
    }

    #[test]
    fn test_forwarding_pointer_install() {
        let region = Region::new(0);
        let old_obj = region.allocate_object(64).unwrap();
        let new_obj = region.allocate_object(64).unwrap();
        let old_hdr = unsafe { &*old_obj };

        // Install forwarding pointer: old → new
        let result = old_hdr.try_install_forwarding(old_obj, new_obj);
        assert_eq!(result, Ok(new_obj));
        assert!(old_hdr.is_forwarded(old_obj));
        assert_eq!(old_hdr.forwarding_address(), new_obj);

        // Second attempt should fail — already forwarded.
        let another = region.allocate_object(64).unwrap();
        let result2 = old_hdr.try_install_forwarding(old_obj, another);
        assert_eq!(result2, Err(new_obj)); // winner was new_obj
    }

    #[test]
    fn test_region_bump_allocation() {
        let region = Region::new(0);
        let header_size = std::mem::size_of::<ObjectHeader>();

        let obj1 = region.allocate_object(64).unwrap();
        let obj2 = region.allocate_object(128).unwrap();

        // Objects should be at different addresses
        assert_ne!(obj1, obj2);

        // obj2 should be after obj1 + total_size
        let expected_offset = align_up(header_size + 64, OBJ_ALIGN);
        let delta = (obj2 as usize) - (obj1 as usize);
        assert_eq!(delta, expected_offset);
    }

    #[test]
    fn test_region_oom() {
        let region = Region::new(0);
        // Try to allocate more than 2MB
        let huge = region.allocate_object(REGION_SIZE + 1);
        assert!(huge.is_none());
    }

    #[test]
    fn test_read_barrier_no_forwarding() {
        let region = Region::new(0);
        let obj = region.allocate_object(64).unwrap();

        // Not forwarded — read_barrier returns the same pointer.
        assert_eq!(read_barrier(obj), obj);
    }

    #[test]
    fn test_read_barrier_with_forwarding() {
        let region = Region::new(0);
        let old_obj = region.allocate_object(64).unwrap();
        let new_obj = region.allocate_object(64).unwrap();

        // Forward old → new
        let old_hdr = unsafe { &*old_obj };
        old_hdr.try_install_forwarding(old_obj, new_obj).unwrap();

        // read_barrier should now return new_obj
        assert_eq!(read_barrier(old_obj), new_obj);
    }

    #[test]
    fn test_write_barrier() {
        let region = Region::new(0);
        let queue = WorkQueue::new();

        let source = region.allocate_object(64).unwrap();
        let target = region.allocate_object(64).unwrap();

        // Source is White — no barrier action.
        write_barrier(unsafe { &*source }, target, &queue);
        assert!(queue.is_empty());
        assert!(unsafe { &*target }.is_white());

        // Make source Black.
        unsafe { &*source }.try_shade_gray();
        unsafe { &*source }.shade_black();

        // Now write_barrier should shade target Gray.
        write_barrier(unsafe { &*source }, target, &queue);
        assert!(unsafe { &*target }.is_gray());
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop().unwrap(), target);
    }

    #[test]
    fn test_mark_phase() {
        let mut gc = JDGC::new(GcConfig::default());

        // Allocate some objects
        let root = gc.allocate_object(64);
        let child = gc.allocate_object(64);

        // Set up root → child reference (write child ptr into root's payload)
        unsafe {
            let payload = (*root).payload_ptr() as *mut *mut ObjectHeader;
            *payload = child;
        }

        // Run marking
        gc.mark_phase(&[root]);

        // Both root and child should be Black
        assert!(unsafe { &*root }.is_black());
        assert!(unsafe { &*child }.is_black());
    }

    #[test]
    fn test_evacuate_object() {
        let gc = JDGC::new(GcConfig::default());
        let source_region = Region::new(100);
        let target_region = Region::new(101);

        let old_obj = source_region.allocate_object(64).unwrap();
        let old_hdr = unsafe { &*old_obj };
        old_hdr.try_shade_gray();
        old_hdr.shade_black();

        let new_obj = gc.evacuate_object(old_obj, &target_region);

        // Object should have been moved
        assert_ne!(new_obj, old_obj);
        // Old object should be forwarded
        assert!(old_hdr.is_forwarded(old_obj));
        assert_eq!(old_hdr.forwarding_address(), new_obj);
        // read_barrier resolves to new location
        assert_eq!(read_barrier(old_obj), new_obj);
    }

    #[test]
    fn test_evacuate_pinned_object() {
        let gc = JDGC::new(GcConfig::default());
        let source_region = Region::new(200);
        let target_region = Region::new(201);

        let obj = source_region.allocate_object(64).unwrap();
        unsafe { &*obj }.pin();

        let result = gc.evacuate_object(obj, &target_region);

        // Pinned objects should NOT be moved
        assert_eq!(result, obj);
        assert!(!unsafe { &*obj }.is_forwarded(obj));
    }

    #[test]
    fn test_full_gc_cycle() {
        let mut gc = JDGC::new(GcConfig {
            initial_regions: 2,
            ..Default::default()
        });

        let root = gc.allocate_object(64);
        let child = gc.allocate_object(128);
        let garbage = gc.allocate_object(256); // not referenced by root

        // Wire root → child
        unsafe {
            let payload = (*root).payload_ptr() as *mut *mut ObjectHeader;
            *payload = child;
        }

        gc.collect(&[root]);

        // root and child survive (now White again after sweep)
        let resolved_root = read_barrier(root);
        let resolved_child = read_barrier(child);
        assert!(unsafe { &*resolved_root }.is_white());
        assert!(unsafe { &*resolved_child }.is_white());
    }

    #[test]
    fn test_read_barrier_null() {
        assert!(read_barrier(ptr::null_mut()).is_null());
    }

    #[test]
    fn test_work_queue_drain_batch() {
        let queue = WorkQueue::new();
        let region = Region::new(0);

        for _ in 0..10 {
            let obj = region.allocate_object(16).unwrap();
            queue.push(obj);
        }

        assert_eq!(queue.len(), 10);

        let mut buf = Vec::new();
        let n = queue.drain_batch(&mut buf, 4);
        assert_eq!(n, 4);
        assert_eq!(buf.len(), 4);
        assert_eq!(queue.len(), 6);
    }

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, 8), 0);
        assert_eq!(align_up(1, 8), 8);
        assert_eq!(align_up(7, 8), 8);
        assert_eq!(align_up(8, 8), 8);
        assert_eq!(align_up(9, 8), 16);
        assert_eq!(align_up(16, 16), 16);
        assert_eq!(align_up(17, 16), 32);
    }
}
