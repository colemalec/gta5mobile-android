// Android controllers and touch controls are translated by GameActivity.java.
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use wasmtime::SharedMemory;

pub static ACTIVE: AtomicBool = AtomicBool::new(false);
pub static IN_VEHICLE: AtomicBool = AtomicBool::new(false);
pub static HORN_PAD: AtomicBool = AtomicBool::new(false);
pub static HORN_TOUCH: AtomicBool = AtomicBool::new(false);

pub fn horn_held() -> bool {
    HORN_PAD.load(Ordering::Relaxed) || HORN_TOUCH.load(Ordering::Relaxed)
}

pub fn poll() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

// --- host-provided gamepad -------------------------------------------------
// The engine's ioPad::Update clears every pad and only fills it from DirectInput
// devices, which this build never has. muguet_pad runs right after that, so the
// engine sees an ordinary connected pad 0 with analog sticks and triggers.
// Address of pad 0 and the field offsets are those of the pinned game.wasm.
const PAD0: u64 = 12_840_656;
const PAD_SIZE: u64 = 136;

static PAD_ON: AtomicBool = AtomicBool::new(false);
// bits 0-15 buttons, then LX LY RX RY L2 R2 as bytes (0x80 = centred).
static PAD_PACKED: AtomicU64 = AtomicU64::new(0x0000_8080_8080_0000);

fn axis_byte(v: f32) -> u64 {
    if !v.is_finite() { return 0x80; }
    // Engine sticks: 0 = left/up, 255 = right/down, which is Android's convention.
    (127.5 + v.clamp(-1.0, 1.0) * 127.5).round() as u64
}
fn trigger_byte(v: f32) -> u64 {
    if !v.is_finite() { return 0; }
    (v.clamp(0.0, 1.0) * 255.0).round() as u64
}

/// Engine pad bits: L2 1, R2 2, L1 4, R1 8, Y 16, B 32, A 64, X 128, Select 256,
/// L3 512, R3 1024, Start 2048, D-pad up 4096, right 8192, down 16384, left 32768.
pub fn update(active: bool, buttons: u32, sticks: [f32; 4], lt: f32, rt: f32) {
    let packed = (buttons as u64 & 0xffff)
        | axis_byte(sticks[0]) << 16
        | axis_byte(sticks[1]) << 24
        | axis_byte(sticks[2]) << 32
        | axis_byte(sticks[3]) << 40
        | trigger_byte(lt) << 48
        | trigger_byte(rt) << 56;
    PAD_PACKED.store(packed, Ordering::Relaxed);
    PAD_ON.store(active, Ordering::Relaxed);
}

pub fn apply(mem: &SharedMemory, pad: u64) {
    if pad != PAD0 || !PAD_ON.load(Ordering::Relaxed) { return; }
    let (base, len) = crate::mem_base_len(mem);
    if pad.saturating_add(PAD_SIZE) > len as u64 { return; }
    let packed = PAD_PACKED.load(Ordering::Relaxed);
    let (l2, r2) = ((packed >> 48) as u8, (packed >> 56) as u8);
    let mut bits = (packed & 0xffff) as u32;
    // Analog triggers also count as pressed buttons past a small threshold.
    if l2 > 30 { bits |= 1; }
    if r2 > 30 { bits |= 2; }
    unsafe {
        let p = base.add(pad as usize);
        (&*(p.add(12) as *const AtomicU32)).fetch_or(bits, Ordering::SeqCst);
        for i in 0..4usize {
            (&*(p.add(20 + i) as *const AtomicU8)).store((packed >> (16 + 8 * i)) as u8, Ordering::SeqCst);
        }
        (&*(p.add(24) as *const AtomicU8)).store(l2, Ordering::SeqCst);
        (&*(p.add(25) as *const AtomicU8)).store(r2, Ordering::SeqCst);
    }
}
