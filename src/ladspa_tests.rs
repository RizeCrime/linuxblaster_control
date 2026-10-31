//! Host-side regression tests for the vendored g6ct LADSPA plugin.
//!
//! These load the shared object that `build.rs` produced — the same bytes the
//! installer writes to `~/.local/lib/linuxblaster/` — and drive it through the
//! LADSPA 1.1 ABI, so the DSP is tested as it ships instead of only checking
//! that the ELF exports an entry point.
//!
//! The coverage mirrors `g6-re` 68fd284 `port/test_ir.c`:
//!
//! * the deterministic 48 kHz render hashed with FNV-1a (the same golden
//!   value as the reference build),
//! * channel-mask gating (silent channels are a no-op, active ones are not),
//! * the stereo-bypass branch (mask `0x3` returns raw FL/FR bit-for-bit).
//!
//! A re-vendored or hand-edited plugin cannot pass these by accident.

#![cfg(all(target_os = "linux", not(target_env = "musl")))]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::mem::transmute;

/// `RTLD_NOW` from `<dlfcn.h>`.
const RTLD_NOW: c_int = 2;

/// LADSPA port indices, from `vendor/g6ct/g6ct_ladspa.c`.
const P_OUTL: u64 = 8;
const P_OUTR: u64 = 9;
const P_MASK: u64 = 10;
const PORT_COUNT: u64 = 11;

/// `G6CT_MASK_7_1`: FL FR FC LFE BL BR SL SR.
const MASK_7_1: f32 = 0x063F as f32;
/// `G6CT_FL | G6CT_FR`: the DLL's stereo-bypass branch.
const MASK_STEREO: f32 = 0x3 as f32;

/// Golden FNV-1a hash of the float32 render. Identical to
/// `GOLDEN_EXPECT` in `g6-re` `port/test_ir.c` for the f64 build, which is
/// itself the DLL's hash for the float32 build.
const GOLDEN_RENDER: u64 = 0x5f78_7985_36b3_fa72;
const FNV_OFFSET: u64 = 1_469_598_103_934_665_603;
const FNV_PRIME: u64 = 1_099_511_628_211;

#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *mut c_char;
}

fn last_dl_error() -> String {
    unsafe {
        let error = dlerror();
        if error.is_null() {
            "no dlerror() message".to_owned()
        } else {
            CStr::from_ptr(error).to_string_lossy().into_owned()
        }
    }
}

/// One LADSPA port range hint, layout-matched to `g6ct_ladspa.c`.
#[repr(C)]
struct PortRangeHint {
    hint_descriptor: u64,
    lower_bound: f32,
    upper_bound: f32,
}

type Instantiate = unsafe extern "C" fn(*const Descriptor, u64) -> *mut c_void;
type ConnectPort = unsafe extern "C" fn(*mut c_void, u64, *mut f32);
type HandleFn = unsafe extern "C" fn(*mut c_void);
type Run = unsafe extern "C" fn(*mut c_void, u64);

/// The LADSPA 1.1 descriptor, layout-matched to `g6ct_ladspa.c`.
/// Fields the tests do not read are still part of the ABI layout.
#[repr(C)]
#[allow(dead_code)]
struct Descriptor {
    unique_id: u64,
    label: *const c_char,
    properties: c_int,
    name: *const c_char,
    maker: *const c_char,
    copyright: *const c_char,
    port_count: u64,
    port_descriptors: *const c_int,
    port_names: *const *const c_char,
    port_range_hints: *const PortRangeHint,
    implementation_data: *mut c_void,
    instantiate: Option<Instantiate>,
    connect_port: Option<ConnectPort>,
    activate: Option<HandleFn>,
    run: Option<Run>,
    run_adding: Option<Run>,
    set_run_adding_gain: Option<unsafe extern "C" fn(*mut c_void, f32)>,
    deactivate: Option<HandleFn>,
    cleanup: Option<HandleFn>,
}

type DescriptorFn = unsafe extern "C" fn(u64) -> *const Descriptor;

/// An opened plugin. Keeps the handle alive for as long as the descriptor is
/// borrowed, and unloads it on drop.
struct Library {
    handle: *mut c_void,
    descriptor: *const Descriptor,
}

impl Library {
    fn open() -> Self {
        let path = concat!(env!("OUT_DIR"), "/libg6ct_ladspa_f64.so");
        let c_path = CString::new(path).unwrap();
        unsafe {
            let handle = dlopen(c_path.as_ptr(), RTLD_NOW);
            assert!(!handle.is_null(), "dlopen({path}) failed: {}", last_dl_error());

            let symbol = dlsym(handle, c"ladspa_descriptor".as_ptr());
            assert!(
                !symbol.is_null(),
                "ladspa_descriptor missing from {path}: {}",
                last_dl_error()
            );

            let descriptor_fn: DescriptorFn = transmute(symbol);
            let descriptor = descriptor_fn(0);
            assert!(!descriptor.is_null(), "ladspa_descriptor(0) returned NULL");

            Self {
                handle,
                descriptor,
            }
        }
    }

    fn descriptor(&self) -> &Descriptor {
        unsafe { &*self.descriptor }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            dlclose(self.handle);
        }
    }
}

/// Renders `inputs` (8 channels, same length) through a fresh instance.
///
/// Returns the two output channels. The instance is created, connected and
/// destroyed inside the call, so no port pointer outlives its buffer.
fn render(
    library: &Library,
    inputs: &mut [Vec<f32>; 8],
    mask: f32,
) -> (Vec<f32>, Vec<f32>) {
    let descriptor = library.descriptor();
    let frames = inputs[0].len();
    let mut out_l = vec![0.0f32; frames];
    let mut out_r = vec![0.0f32; frames];
    let mut mask = mask;

    unsafe {
        let instantiate = descriptor.instantiate.expect("instantiate");
        let connect_port = descriptor.connect_port.expect("connect_port");
        let activate = descriptor.activate.expect("activate");
        let run = descriptor.run.expect("run");
        let cleanup = descriptor.cleanup.expect("cleanup");

        let instance = instantiate(descriptor, 48_000);
        assert!(!instance.is_null(), "instantiate() returned NULL");

        for channel in 0..8u64 {
            connect_port(
                instance,
                channel,
                inputs[channel as usize].as_mut_ptr(),
            );
        }
        connect_port(instance, P_OUTL, out_l.as_mut_ptr());
        connect_port(instance, P_OUTR, out_r.as_mut_ptr());
        connect_port(instance, P_MASK, &mut mask);

        activate(instance);
        run(instance, frames as u64);
        cleanup(instance);
    }

    (out_l, out_r)
}

/// g6-re's integer-only LCG. Keeping it integer-only means the input bits do
/// not depend on libm, so the render is reproducible across machines.
fn fill_noise(inputs: &mut [Vec<f32>; 8], state: &mut u32) {
    for frame in 0..inputs[0].len() {
        for channel in inputs.iter_mut() {
            *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let value = (*state >> 8) as i32 - 8_388_608;
            channel[frame] = (f64::from(value) * (1.0 / 8_388_608.0)) as f32;
        }
    }
}

fn noise_inputs(frames: usize, state: &mut u32) -> [Vec<f32>; 8] {
    let mut inputs = std::array::from_fn(|_| vec![0.0f32; frames]);
    fill_noise(&mut inputs, state);
    inputs
}

fn fnv_u32(hash: u64, value: u32) -> u64 {
    (hash ^ u64::from(value)).wrapping_mul(FNV_PRIME)
}

fn render_hash(left: &[f32], right: &[f32]) -> u64 {
    let mut hash = FNV_OFFSET;
    for (l, r) in left.iter().zip(right) {
        hash = fnv_u32(hash, l.to_bits());
        hash = fnv_u32(hash, r.to_bits());
    }
    hash
}

#[test]
fn descriptor_matches_the_vendored_source() {
    let library = Library::open();
    let descriptor = library.descriptor();

    unsafe {
        assert_eq!(CStr::from_ptr(descriptor.label).to_str().unwrap(), "g6ct_matrix_encoder");
        assert_eq!(descriptor.port_count, PORT_COUNT);
        assert_eq!(descriptor.unique_id, 0x6736_5431);
        assert_eq!(CStr::from_ptr(descriptor.copyright).to_str().unwrap(), "MIT");

        let names: Vec<&str> = (0..PORT_COUNT)
            .map(|port| {
                CStr::from_ptr(*descriptor.port_names.add(port as usize))
                    .to_str()
                    .unwrap()
            })
            .collect();
        assert_eq!(
            names,
            [
                "FL", "FR", "FC", "LFE", "BL", "BR", "SL", "SR", "OutL", "OutR",
                "Channel Mask"
            ]
        );

        // 0x5 = LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL
        assert_eq!(*descriptor.port_descriptors.add(P_MASK as usize), 0x5);
        // 0xA = LADSPA_PORT_OUTPUT | LADSPA_PORT_AUDIO
        assert_eq!(*descriptor.port_descriptors.add(P_OUTL as usize), 0xA);

        let hint = &*descriptor.port_range_hints.add(P_MASK as usize);
        // BOUNDED_BELOW | BOUNDED_ABOVE | INTEGER
        assert_eq!(hint.hint_descriptor, 0x2 | 0x4 | 0x20);
        assert_eq!(hint.lower_bound, 0.0);
        assert_eq!(hint.upper_bound, 4095.0);
    }
}

#[test]
fn deterministic_render_matches_the_g6_re_golden_hash() {
    let library = Library::open();
    // One second at 48 kHz, in the same 1024-frame blocks as g6-re's test.
    let mut state = 0x6736_5431u32;
    let mut inputs = noise_inputs(48_000, &mut state);
    let (left, right) = render(&library, &mut inputs, MASK_7_1);

    let hash = render_hash(&left, &right);
    assert_eq!(
        hash, GOLDEN_RENDER,
        "golden render drifted: 0x{hash:016x} != 0x{GOLDEN_RENDER:016x}"
    );
}

#[test]
fn front_impulse_has_the_expected_gain() {
    let library = Library::open();
    let mut inputs: [Vec<f32>; 8] = std::array::from_fn(|_| vec![0.0f32; 64]);
    inputs[0][0] = 1.0;

    let (left, right) = render(&library, &mut inputs, MASK_7_1);

    // FL goes through the two front all-pass sections; the surround path is
    // silent, so the right channel sees only the (zero) mirrored input.
    assert!(
        (left[0] as f64 - 0.060_466_885_6).abs() < 1e-6,
        "first sample {}",
        left[0]
    );
    assert!(
        (left[1] as f64 + 0.748_966_992).abs() < 1e-6,
        "second sample {}",
        left[1]
    );
    assert!(right.iter().all(|sample| sample.abs() < 1e-9));
}

#[test]
fn stereo_bypass_returns_raw_front_channels() {
    let library = Library::open();
    let mut state = 0x6736_5431u32;
    let mut inputs = noise_inputs(1024, &mut state);
    let expected_l = inputs[0].clone();
    let expected_r = inputs[1].clone();

    let (left, right) = render(&library, &mut inputs, MASK_STEREO);

    assert_eq!(left, expected_l, "mask 0x3 must not filter FL");
    assert_eq!(right, expected_r, "mask 0x3 must not filter FR");
}

#[test]
fn mask_zero_selects_the_7_1_default() {
    let library = Library::open();
    let mut state = 0x6736_5431u32;
    let mut inputs = noise_inputs(1024, &mut state);
    let mut reference = inputs.clone();

    let (default_l, default_r) = render(&library, &mut reference, MASK_7_1);
    let (zero_l, zero_r) = render(&library, &mut inputs, 0.0);

    assert_eq!(zero_l, default_l, "mask 0 must mean 7.1");
    assert_eq!(zero_r, default_r, "mask 0 must mean 7.1");
}

#[test]
fn mask_gates_the_channels_it_clears() {
    let library = Library::open();
    let mut state = 0x6736_5431u32;
    let mut full = noise_inputs(1024, &mut state);
    let (base_l, base_r) = render(&library, &mut full, MASK_7_1);

    // Clearing a channel that carries signal must change the render.
    for (bit, name) in [
        (0x0004u32, "FC"),
        (0x0008, "LFE"),
        (0x0010, "BL"),
        (0x0020, "BR"),
        (0x0200, "SL"),
        (0x0400, "SR"),
    ] {
        let mut inputs = full.clone();
        let (left, right) =
            render(&library, &mut inputs, (0x063F & !bit) as f32);
        assert!(
            left != base_l || right != base_r,
            "clearing {name} left the render unchanged"
        );
    }

    // Clearing a channel that is silent must be a no-op.
    let mut silent = full.clone();
    silent[4].fill(0.0); // BL
    silent[5].fill(0.0); // BR
    let (quiet_l, quiet_r) = render(&library, &mut silent, MASK_7_1);
    let (gated_l, gated_r) =
        render(&library, &mut silent, (0x063F & !0x0030) as f32);
    assert_eq!(gated_l, quiet_l, "silent BL/BR gating must be a no-op");
    assert_eq!(gated_r, quiet_r, "silent BL/BR gating must be a no-op");
}

#[test]
fn out_of_range_mask_values_follow_the_documented_fallback() {
    let library = Library::open();
    let mut state = 0x6736_5431u32;
    let mut inputs = noise_inputs(256, &mut state);
    let mut reference = inputs.clone();

    let (default_l, default_r) = render(&library, &mut reference, MASK_7_1);
    for fallback in [-1.0f32, f32::NAN] {
        let (left, right) = render(&library, &mut inputs, fallback);
        assert_eq!(left, default_l, "mask {fallback} must fall back to 7.1");
        assert_eq!(right, default_r, "mask {fallback} must fall back to 7.1");
    }

    // Above 4095 the wrapper clamps instead of wrapping. With only the eight
    // connected channels, 4095 selects the same audible taps as 0x63F.
    let (clamped_l, clamped_r) = render(&library, &mut inputs, 5000.0);
    assert_eq!(clamped_l, default_l, "mask above 4095 must clamp");
    assert_eq!(clamped_r, default_r, "mask above 4095 must clamp");
}
