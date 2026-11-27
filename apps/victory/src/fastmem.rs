//! `memcmp` for this executable, in place of the C runtime's.
//!
//! On `x86_64-pc-windows-gnu` Rust's slice comparisons call `memcmp` from
//! the C runtime, and under Wine that is ucrtbase's, which compares eight
//! bytes a step: 0.16 of a core in the intro scene (perf), most of it on the
//! threads that refresh host copies and land what shaders wrote, which the
//! submitting thread waits for. This one compares 32 bytes a step with AVX2.
//! The executable's own definition wins over the import library's, for
//! every crate linked into it.

use std::arch::x86_64::{__m256i, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_movemask_epi8};
use std::sync::atomic::{AtomicU8, Ordering};

/// 0 not asked yet, 1 without AVX2, 2 with.
static AVX2: AtomicU8 = AtomicU8::new(0);

#[inline]
fn avx2() -> bool {
    match AVX2.load(Ordering::Relaxed) {
        0 => {
            let has = std::arch::is_x86_feature_detected!("avx2");
            AVX2.store(if has { 2 } else { 1 }, Ordering::Relaxed);
            has
        }
        v => v == 2,
    }
}

/// # Safety
/// As C's `memcmp`: `a` and `b` readable for `n` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const core::ffi::c_void, b: *const core::ffi::c_void, n: usize) -> i32 {
    let (a, b) = (a.cast::<u8>(), b.cast::<u8>());
    // SAFETY: as the caller promises.
    unsafe {
        if n >= 32 && avx2() {
            by_vectors(a, b, n)
        } else {
            by_words(a, b, n, 0)
        }
    }
}

#[target_feature(enable = "avx2")]
unsafe fn by_vectors(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i + 32 <= n {
        // SAFETY: inside the n bytes.
        let same = unsafe {
            let x = _mm256_loadu_si256(a.add(i).cast::<__m256i>());
            let y = _mm256_loadu_si256(b.add(i).cast::<__m256i>());
            _mm256_movemask_epi8(_mm256_cmpeq_epi8(x, y)) as u32
        };
        if same != u32::MAX {
            let at = i + (!same).trailing_zeros() as usize;
            // SAFETY: inside the n bytes.
            return unsafe { i32::from(*a.add(at)) - i32::from(*b.add(at)) };
        }
        i += 32;
    }
    // SAFETY: as the caller promises.
    unsafe { by_words(a, b, n, i) }
}

/// From byte `i` on, eight bytes a step, then one. (Volatile reads: the
/// compiler must not see a loop it would make a call to `memcmp` of.)
#[inline]
unsafe fn by_words(a: *const u8, b: *const u8, n: usize, mut i: usize) -> i32 {
    while i + 8 <= n {
        // SAFETY: inside the n bytes.
        let (x, y) = unsafe {
            (a.add(i).cast::<u64>().read_unaligned(), b.add(i).cast::<u64>().read_unaligned())
        };
        if x != y {
            // Little-endian: the lowest differing byte is the first.
            let at = i + ((x ^ y).trailing_zeros() / 8) as usize;
            // SAFETY: inside the n bytes.
            return unsafe { i32::from(*a.add(at)) - i32::from(*b.add(at)) };
        }
        i += 8;
    }
    while i < n {
        // SAFETY: inside the n bytes.
        let (x, y) = unsafe { (a.add(i).read_volatile(), b.add(i).read_volatile()) };
        if x != y {
            return i32::from(x) - i32::from(y);
        }
        i += 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::memcmp;

    fn reference(a: &[u8], b: &[u8]) -> i32 {
        for (x, y) in a.iter().zip(b) {
            if x != y {
                return i32::from(*x) - i32::from(*y);
            }
        }
        0
    }

    #[test]
    fn agrees_with_a_plain_compare() {
        let mut seed = 0x1234_5678_9abc_def0u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for n in 0..200usize {
            for _ in 0..50 {
                let a: Vec<u8> = (0..n).map(|_| (next() % 4) as u8).collect();
                let mut b = a.clone();
                if n > 0 && next() % 3 != 0 {
                    let at = (next() as usize) % n;
                    b[at] = (next() % 256) as u8;
                }
                // SAFETY: both hold n bytes.
                let got = unsafe { memcmp(a.as_ptr().cast(), b.as_ptr().cast(), n) };
                assert_eq!(got.signum(), reference(&a, &b).signum(), "n {n}");
            }
        }
    }
}
