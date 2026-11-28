//! Buffers that chosen compute programs read and write, saved at the end of
//! a captured frame (diagnostics): with `VICTORY_BUFDUMP=<hash>,...` (16-digit
//! shader hashes as the pipeline diagnostics print them) and a capture folder
//! (`VICTORY_CAPTURE`), every buffer view in the tables of a dispatch of such
//! a program in a frame [`crate::capture::listed`] names is noted; at that
//! frame's present, once every submission has run, the bytes of each noted
//! range are read back from its host copy (what the next shader that reads it
//! sees) and written to `frame-<n>-buf-<hash>-<u|s>-<address>-f<format>.bin`,
//! with a line in the log on what they hold read as the view's format.
//!
//! Made to find what the four programs that make heads vanish write
//! (CLAUDE.md, 7c): NaN, huge values or nothing.

use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::object::This;
use crate::shadow::Shadow;

/// The hashes asked for.
fn hashes() -> &'static [String] {
    static HASHES: OnceLock<Vec<String>> = OnceLock::new();
    HASHES.get_or_init(|| {
        std::env::var("VICTORY_BUFDUMP")
            .unwrap_or_default()
            .split(',')
            .map(|h| h.trim().to_ascii_lowercase())
            .filter(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .collect()
    })
}

/// Of pipeline `this`: the asked-for shader hash it has, if the frame being
/// recorded is one to dump.
pub(crate) fn watched(this: This) -> Option<u8> {
    let hashes = hashes();
    if hashes.is_empty() || this.is_null() || crate::capture::folder().is_none() {
        return None;
    }
    if !crate::capture::listed(crate::queue::presented() + 1) {
        return None;
    }
    crate::pipeline::shader_among(this, hashes).map(|i| i as u8)
}

struct Noted {
    hash: u8,
    unordered: bool,
    format: u32,
    shadow: Arc<Shadow>,
    address: u64,
    len: u64,
    /// Dispatches that bound it.
    uses: u32,
}

static NOTED: Mutex<Vec<Noted>> = Mutex::new(Vec::new());

/// Notes a buffer view of a watched dispatch.
pub(crate) fn note(hash: u8, unordered: bool, format: u32, shadow: &Arc<Shadow>, address: u64, len: u64) {
    let mut noted = NOTED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = noted
        .iter_mut()
        .find(|n| n.hash == hash && n.unordered == unordered && n.address == address && n.len == len && n.format == format)
    {
        n.uses += 1;
        return;
    }
    if noted.len() < 64 {
        noted.push(Noted { hash, unordered, format, shadow: shadow.clone(), address, len, uses: 1 });
    }
}

/// At the present of `frame` (every submission has run): saves the noted
/// ranges.
pub(crate) fn dump(frame: u64) {
    let noted = std::mem::take(&mut *NOTED.lock().unwrap_or_else(|e| e.into_inner()));
    let (Some(host), Some(folder)) = (crate::host::host(), crate::capture::folder()) else {
        return;
    };
    const MOST: u64 = 64 << 20;
    for n in &noted {
        let len = n.len.min(MOST);
        let Some(bytes) = read(host, &n.shadow, n.address, len) else {
            tracing::warn!(address = format_args!("{:#x}", n.address), "bufdump: read back failed");
            continue;
        };
        let hash = &hashes()[n.hash as usize];
        let kind = if n.unordered { "u" } else { "s" };
        let name = format!("frame-{frame}-buf-{hash}-{kind}-{:x}-f{}.bin", n.address, n.format);
        let _ = std::fs::write(folder.join(&name), &bytes);
        tracing::info!(
            frame,
            hash = hash.as_str(),
            kind,
            address = format_args!("{:#x}", n.address),
            len = n.len,
            format = n.format,
            uses = n.uses,
            in_place = n.shadow.in_place(),
            summary = %summary(n.format, &bytes),
            "bufdump: {name}"
        );
    }
}

/// The bytes `[address, address + len)` of `shadow`'s host buffer.
fn read(host: &crate::host::Host, shadow: &Shadow, address: u64, len: u64) -> Option<Vec<u8>> {
    let heap = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_READBACK, ..Default::default() };
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: len,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        ..Default::default()
    };
    let mut back: Option<ID3D12Resource> = None;
    // SAFETY: plain creation.
    unsafe {
        host.device
            .CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COPY_DEST, None, &mut back)
            .ok()?;
    }
    let back = back?;
    let list = host.new_list().ok()?;
    let offset = shadow.offset(address);
    let len = len.min(shadow.width().saturating_sub(offset));
    // SAFETY: an open list of ours; live buffers, the range inside both
    // (buffers decay to the common state between submissions).
    unsafe { list.list.CopyBufferRegion(&back, 0, &shadow.buffer, offset, len) };
    list.close();
    host.execute(&[&list]);
    let mut data = std::ptr::null_mut();
    // SAFETY: the copy ran (`execute` waits); the mapping spans `len` bytes.
    unsafe {
        back.Map(0, None, Some(&mut data)).ok()?;
        let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len as usize).to_vec();
        back.Unmap(0, None);
        Some(bytes)
    }
}

fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (h >> 10) & 0x1f;
    let man = f32::from(h & 0x3ff);
    sign * match exp {
        0 => man * 2f32.powi(-24),
        31 if man == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        e => (1.0 + man / 1024.0) * 2f32.powi(i32::from(e) - 15),
    }
}

/// What `bytes` hold: zero share, and read as 16-bit halves and as 32-bit
/// floats, how many are NaN or infinite, the largest finite magnitude, and
/// magnitudes by power of ten.
fn summary(format: u32, bytes: &[u8]) -> String {
    let zero = bytes.chunks(4).filter(|w| w.iter().all(|&b| b == 0)).count();
    let words = bytes.len() / 4;
    let mut out = format!("{} bytes, {:.1}% zero words", bytes.len(), 100.0 * zero as f64 / words.max(1) as f64);
    let mut halves = (0usize, 0usize, 0f32, [0usize; 8]);
    for h in bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])) {
        let v = half(h);
        if v.is_nan() {
            halves.0 += 1;
        } else if v.is_infinite() {
            halves.1 += 1;
        } else {
            halves.2 = halves.2.max(v.abs());
            halves.3[bucket(v)] += 1;
        }
    }
    let mut floats = (0usize, 0usize, 0f32, [0usize; 8]);
    for f in bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])) {
        if f.is_nan() {
            floats.0 += 1;
        } else if f.is_infinite() {
            floats.1 += 1;
        } else {
            floats.2 = floats.2.max(f.abs());
            floats.3[bucket(f)] += 1;
        }
    }
    // Buckets: 0, <1e-4, <1e-2, <1, <1e2, <1e4, <1e6, more.
    out += &format!(
        "; as halves: {} NaN, {} inf, max {:.4e}, by size {:?}; as floats: {} NaN, {} inf, max {:.4e}, by size {:?}",
        halves.0, halves.1, halves.2, halves.3, floats.0, floats.1, floats.2, floats.3
    );
    let _ = format;
    out
}

fn bucket(v: f32) -> usize {
    let a = v.abs();
    match a {
        0.0 => 0,
        a if a < 1e-4 => 1,
        a if a < 1e-2 => 2,
        a if a < 1.0 => 3,
        a if a < 1e2 => 4,
        a if a < 1e4 => 5,
        a if a < 1e6 => 6,
        _ => 7,
    }
}
