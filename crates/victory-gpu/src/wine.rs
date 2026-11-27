//! Whether we run under Wine, and what this fork does about it there.
//!
//! Under Wine every section is a shared mapping of a file. RADV imports host
//! memory as an amdgpu userptr that libdrm makes "anonymous only", so title
//! memory in sections (the page pool) cannot be opened as a heap unless the
//! userptr shim (`linux/userptr-shim`) is loaded; vkd3d-proton then falls
//! back to memory of its own, silently.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Whether ntdll is Wine's (it exports `wine_get_version`).
pub fn on() -> bool {
    static WINE: LazyLock<bool> = LazyLock::new(|| {
        use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
        // SAFETY: plain lookups in ntdll, always loaded.
        unsafe {
            let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
            !ntdll.is_null() && GetProcAddress(ntdll, c"wine_get_version".as_ptr().cast()).is_some()
        }
    });
    *WINE
}

/// The title's page-mapping calls (`xmem`): views mapped, their nanoseconds,
/// views unmapped, theirs. Under Wine each is a wineserver request that
/// walks the process's views; the debug panel shows them. Only grow.
pub static TITLE_MAPS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// Notes one mapping call of `xmem` and how long it took.
pub fn note_map(unmap: bool, nanos: u64) {
    let at = if unmap { 2 } else { 0 };
    TITLE_MAPS[at].fetch_add(1, Ordering::Relaxed);
    TITLE_MAPS[at + 1].fetch_add(nanos, Ordering::Relaxed);
}

/// Whether the userptr shim is preloaded (Wine passes `LD_PRELOAD` on).
pub fn shim() -> bool {
    static SHIM: LazyLock<bool> =
        LazyLock::new(|| std::env::var("LD_PRELOAD").is_ok_and(|v| v.contains("libvictory_userptr")));
    *SHIM
}
