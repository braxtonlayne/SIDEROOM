//! A counting global allocator for memory budget tests (RDY-PERF-004).
//!
//! ```ignore
//! #[global_allocator]
//! static ALLOC: sr_memcount::Counting = sr_memcount::Counting;
//!
//! let before = sr_memcount::live_bytes();
//! let thing = build_it();
//! let held = sr_memcount::live_bytes() - before;
//! ```
//!
//! The count is per thread: an allocation adds to the counter of the thread that made it and a free subtracts
//! from the counter of the thread that frees. A test on a current-thread Tokio runtime therefore sees exactly the
//! bytes its own tasks hold, however many other tests run in the same process. (A value allocated on one thread
//! and dropped on another skews both counters; the gateway budget test keeps everything on one thread.)

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // `const` initial value and no destructor: reading it never allocates, so the allocator can use it.
    static LIVE: Cell<isize> = const { Cell::new(0) };
}

fn add(delta: isize) {
    // `try_with`: a thread that is being torn down has no counter any more; its frees go uncounted.
    let _ = LIVE.try_with(|c| c.set(c.get() + delta));
}

/// Live heap bytes allocated by the current thread (allocations minus frees; can be negative).
pub fn live_bytes() -> isize {
    LIVE.try_with(Cell::get).unwrap_or(0)
}

/// Install with `#[global_allocator]`; forwards to the system allocator.
pub struct Counting;

#[allow(unsafe_code)]
// SAFETY: every method forwards to `System` with the same arguments; the counter updates have no effect on the
// returned memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, which `System` shares.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            add(layout.size() as isize);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as in `alloc`.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            add(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        add(-(layout.size() as isize));
        // SAFETY: `ptr` and `layout` come from a matching allocation, as the caller guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract, which `System` shares.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            add(new_size as isize - layout.size() as isize);
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[global_allocator]
    static ALLOC: Counting = Counting;

    #[test]
    fn counts_what_this_thread_holds() {
        let before = live_bytes();
        let v: Vec<u8> = Vec::with_capacity(10_000);
        assert!(live_bytes() - before >= 10_000);
        drop(v);
        assert!((live_bytes() - before).abs() < 512);
        // Growth through realloc is counted too.
        let mut s = String::new();
        let b2 = live_bytes();
        for _ in 0..1000 {
            s.push_str("0123456789");
        }
        assert!(live_bytes() - b2 >= 10_000);
    }

    #[test]
    fn another_thread_does_not_move_this_counter() {
        let before = live_bytes();
        std::thread::spawn(|| {
            let _big = vec![0u8; 1 << 20];
        })
        .join()
        .unwrap();
        assert!((live_bytes() - before).abs() < 4096);
    }
}
