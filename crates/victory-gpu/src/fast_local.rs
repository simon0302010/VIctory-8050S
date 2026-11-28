//! Thread-locals without std's cost per access on `x86_64-pc-windows-gnu`.
//!
//! There std's `thread_local!` keeps each value behind an OS key
//! (`TlsGetValue`), and every access of a key whose value needs freeing (all
//! of them: the value is boxed) calls `guard::enable`, which increments and
//! decrements one process-wide counter with locked instructions and calls
//! `FlsSetValue`. The title's five recording threads touch thread-locals
//! several times per descriptor they bind and fought over that one cache
//! line: 13.7% of the CPU time in our code (perf, intro scene, run
//! 1795814153), more than any function of ours.
//!
//! [`FastLocal`]: one TLS index per key, and a thread's value boxed on its
//! first access and never freed; an access reads the TEB's slot (no call, no
//! atomic). Only for values that are small, or for threads that live as
//! long as the process: a thread that ends leaks its value. Scoped threads
//! made per call keep std's `thread_local!`.
//!
//! [`fast_local!`] takes `thread_local!`'s syntax; off Windows it is
//! `thread_local!`.

#[cfg(windows)]
use std::cell::{Cell, RefCell};
#[cfg(windows)]
use std::sync::atomic::{AtomicU32, Ordering};

/// `TLS_OUT_OF_INDEXES`: no index yet.
#[cfg(windows)]
const NONE: u32 = u32::MAX;

#[cfg(windows)]
pub(crate) struct FastLocal<T: 'static> {
    key: AtomicU32,
    init: fn() -> T,
}

#[cfg(windows)]
impl<T: 'static> FastLocal<T> {
    pub(crate) const fn new(init: fn() -> T) -> Self {
        FastLocal { key: AtomicU32::new(NONE), init }
    }

    #[inline]
    fn key(&self) -> u32 {
        match self.key.load(Ordering::Acquire) {
            NONE => self.make_key(),
            key => key,
        }
    }

    #[cold]
    fn make_key(&self) -> u32 {
        use windows_sys::Win32::System::Threading::{TlsAlloc, TlsFree};
        // SAFETY: plain calls.
        let new = unsafe { TlsAlloc() };
        assert_ne!(new, NONE, "no TLS index left");
        match self.key.compare_exchange(NONE, new, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                if new >= 64 {
                    tracing::info!(index = new, "a thread-local of ours past the TEB's 64 slots (read through TlsGetValue)");
                }
                new
            }
            Err(other) => {
                // SAFETY: our index, not handed out.
                unsafe { TlsFree(new) };
                other
            }
        }
    }

    /// This thread's value, made on its first access.
    #[inline]
    fn here(&'static self) -> *const T {
        let key = self.key();
        let value = slot(key).cast::<T>();
        if !value.is_null() {
            return value;
        }
        self.first(key)
    }

    #[cold]
    fn first(&'static self, key: u32) -> *const T {
        use windows_sys::Win32::System::Threading::TlsSetValue;
        let value = Box::into_raw(Box::new((self.init)()));
        // SAFETY: an index of ours.
        let set = unsafe { TlsSetValue(key, value.cast()) };
        assert_ne!(set, 0, "TlsSetValue failed");
        value
    }

    /// As `LocalKey::with`.
    #[inline]
    pub(crate) fn with<R>(&'static self, f: impl FnOnce(&T) -> R) -> R {
        // SAFETY: this thread's value, which lives as long as the thread
        // and is never handed to another.
        f(unsafe { &*self.here() })
    }

    /// As `LocalKey::try_with`; it never fails.
    #[inline]
    pub(crate) fn try_with<R>(&'static self, f: impl FnOnce(&T) -> R) -> Result<R, std::convert::Infallible> {
        Ok(self.with(f))
    }
}

#[cfg(windows)]
impl<T: Copy + 'static> FastLocal<Cell<T>> {
    #[inline]
    pub(crate) fn get(&'static self) -> T {
        self.with(Cell::get)
    }
    #[inline]
    pub(crate) fn set(&'static self, value: T) {
        self.with(|c| c.set(value));
    }
    #[inline]
    pub(crate) fn replace(&'static self, value: T) -> T {
        self.with(|c| c.replace(value))
    }
}

#[cfg(windows)]
#[allow(dead_code)]
impl<T: Default + 'static> FastLocal<RefCell<T>> {
    #[inline]
    pub(crate) fn take(&'static self) -> T {
        self.with(|c| c.take())
    }
}

#[cfg(windows)]
#[allow(dead_code)]
impl<T: 'static> FastLocal<RefCell<T>> {
    #[inline]
    pub(crate) fn with_borrow<R>(&'static self, f: impl FnOnce(&T) -> R) -> R {
        self.with(|c| f(&c.borrow()))
    }
    #[inline]
    pub(crate) fn with_borrow_mut<R>(&'static self, f: impl FnOnce(&mut T) -> R) -> R {
        self.with(|c| f(&mut c.borrow_mut()))
    }
}

/// A word per thread kept in the TLS slot itself: nothing is allocated, so
/// the global allocator may use it. 0 until the thread sets it.
#[cfg(windows)]
#[cfg_attr(not(feature = "diagnostics"), allow(dead_code))]
pub(crate) struct FastWord {
    key: AtomicU32,
}

#[cfg(windows)]
#[cfg_attr(not(feature = "diagnostics"), allow(dead_code))]
impl FastWord {
    pub(crate) const fn new() -> Self {
        FastWord { key: AtomicU32::new(NONE) }
    }

    #[inline]
    fn key(&self) -> u32 {
        match self.key.load(Ordering::Acquire) {
            NONE => self.make_key(),
            key => key,
        }
    }

    #[cold]
    fn make_key(&self) -> u32 {
        use windows_sys::Win32::System::Threading::{TlsAlloc, TlsFree};
        // SAFETY: plain calls (Wine's heap, not ours).
        let new = unsafe { TlsAlloc() };
        assert_ne!(new, NONE, "no TLS index left");
        match self.key.compare_exchange(NONE, new, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => new,
            Err(other) => {
                // SAFETY: our index, not handed out.
                unsafe { TlsFree(new) };
                other
            }
        }
    }

    /// This thread's word (0 if it never set one).
    #[inline]
    pub(crate) fn get(&self) -> usize {
        slot(self.key()) as usize
    }

    #[inline]
    pub(crate) fn set(&self, value: usize) {
        use windows_sys::Win32::System::Threading::TlsSetValue;
        // SAFETY: an index of ours.
        unsafe { TlsSetValue(self.key(), value as *const core::ffi::c_void) };
    }
}

/// The TEB's TLS slot `key`: `TlsSlots` (64 of them at gs:0x1480), then
/// `TlsExpansionSlots` (a pointer at gs:0x1780, null until one is set).
#[cfg(windows)]
#[inline]
fn slot(key: u32) -> *mut core::ffi::c_void {
    let value: *mut core::ffi::c_void;
    if key < 64 {
        // SAFETY: the TEB's slots of the running thread.
        unsafe {
            std::arch::asm!(
                "mov {v}, gs:[0x1480 + {k} * 8]",
                v = out(reg) value,
                k = in(reg) u64::from(key),
                options(nostack, readonly, preserves_flags),
            );
        }
    } else {
        // Indices past the TEB's 64 go through `TlsGetValue`: reading
        // `TlsExpansionSlots` (gs:0x1780) ourselves gave null on this stack
        // (Wine, the title's threads), so a key there read as unset at every
        // access, a fresh value each time, and what was set in it was lost.
        // Keys are handed out in first-use order, so which local landed
        // there changed with timing and with every local added: characters'
        // heads came and went from run to run (2026-11-28; adding one
        // fast_local, bufdump's, made them vanish in every run).
        use windows_sys::Win32::System::Threading::TlsGetValue;
        // SAFETY: an index of ours.
        value = unsafe { TlsGetValue(key) };
    }
    value
}

/// `thread_local!`'s syntax, a [`FastLocal`] on Windows.
#[macro_export]
macro_rules! fast_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident : $t:ty = $init:expr; $($rest:tt)*) => {
        $crate::fast_local!($(#[$attr])* $vis static $name: $t = $init);
        $crate::fast_local!($($rest)*);
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident : $t:ty = $init:expr) => {
        #[cfg(windows)]
        $(#[$attr])* $vis static $name: $crate::fast_local::FastLocal<$t> =
            $crate::fast_local::FastLocal::new(|| $init);
        #[cfg(not(windows))]
        std::thread_local!($(#[$attr])* $vis static $name: $t = $init);
    };
}

#[cfg(all(test, windows))]
mod tests {
    use std::cell::Cell;

    crate::fast_local! {
        static A: Cell<u32> = const { Cell::new(7) };
        static B: std::cell::RefCell<Vec<u32>> = std::cell::RefCell::new(vec![1]);
    }

    #[test]
    fn each_thread_has_its_own() {
        assert_eq!(A.get(), 7);
        A.set(9);
        assert_eq!(A.replace(10), 9);
        std::thread::spawn(|| assert_eq!(A.get(), 7)).join().unwrap();
        assert_eq!(A.get(), 10);
        B.with_borrow_mut(|b| b.push(2));
        assert_eq!(B.with_borrow(|b| b.clone()), [1, 2]);
    }

    #[test]
    fn a_word_is_kept_per_thread() {
        static W: super::FastWord = super::FastWord::new();
        assert_eq!(W.get(), 0);
        W.set(5);
        std::thread::spawn(|| assert_eq!(W.get(), 0)).join().unwrap();
        assert_eq!(W.get(), 5);
    }

    #[test]
    fn many_keys_reach_the_expansion_slots() {
        // Indexes past 64 live in the expansion slots.
        let mut held = Vec::new();
        for _ in 0..80 {
            // SAFETY: plain calls.
            held.push(unsafe { windows_sys::Win32::System::Threading::TlsAlloc() });
        }
        crate::fast_local!(static C: Cell<u64> = const { Cell::new(3) });
        assert_eq!(C.get(), 3);
        C.set(4);
        assert_eq!(C.get(), 4);
        for key in held {
            // SAFETY: ours.
            unsafe { windows_sys::Win32::System::Threading::TlsFree(key) };
        }
    }
}
