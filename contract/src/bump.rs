//! Bump allocator over a fixed static heap.
//!
//! A contract call is short-lived and single-threaded, so a bump allocator is enough. The heap
//! lives in `.bss`, which pallet-revive counts against the contract's static memory limit
//! (blob size + rw data + stack <= 1.5 MiB), so its size is a trade-off with code size.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

const HEAP_SIZE: usize = 768 * 1024;

struct Heap {
    mem: UnsafeCell<[u8; HEAP_SIZE]>,
    next: UnsafeCell<usize>,
}

// Safety: PolkaVM contracts are single-threaded.
unsafe impl Sync for Heap {}

static HEAP: Heap = Heap {
    mem: UnsafeCell::new([0; HEAP_SIZE]),
    next: UnsafeCell::new(0),
};

pub struct Bump;

unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let base = HEAP.mem.get() as usize;
            let next = &mut *HEAP.next.get();
            let start = (base + *next).next_multiple_of(layout.align());
            let end = start + layout.size();
            if end > base + HEAP_SIZE {
                return core::ptr::null_mut();
            }
            *next = end - base;
            start as *mut u8
        }
    }

    // Freeing the most recent allocation rolls the bump pointer back; anything else leaks.
    // This reclaims the many short-lived temporaries in the verifier's inner loops.
    #[inline(never)] // keeps the blob small: this is called from every drop site
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            let base = HEAP.mem.get() as usize;
            let next = &mut *HEAP.next.get();
            if ptr as usize + layout.size() == base + *next {
                *next = ptr as usize - base;
            }
        }
    }

    // Growing the most recent allocation in place avoids copying large buffers
    // (e.g. vectors built with repeated pushes).
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe {
            let base = HEAP.mem.get() as usize;
            let next = &mut *HEAP.next.get();
            if ptr as usize + layout.size() == base + *next
                && ptr as usize + new_size <= base + HEAP_SIZE
            {
                *next = ptr as usize + new_size - base;
                return ptr;
            }
            let new = self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
            if !new.is_null() {
                core::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
            }
            new
        }
    }
}
