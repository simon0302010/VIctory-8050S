//! Whether we run under Wine, and what this fork does about it there.
//!
//! Under Wine every section is a shared mapping of a file. RADV imports host
//! memory as an amdgpu userptr that libdrm makes "anonymous only", so title
//! memory in sections (the page pool) cannot be opened as a heap unless the
//! userptr shim (`linux/userptr-shim`) is loaded; vkd3d-proton then falls
//! back to memory of its own, silently.

use std::sync::LazyLock;

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
