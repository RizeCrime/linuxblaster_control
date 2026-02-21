// pcapng parser for SoundBlasterX G6 USB HID captures.
//
// Usage:  cargo run -- captures/<file>.pcapng > parsed/<file>.json
// Output: JSON array of decoded URB packets to stdout.

#![allow(dead_code)]

use std::env;
use std::fs::{self, File};
use std::io::BufWriter;

use pcap_file::pcapng::PcapNgReader;
use pcap_file::pcapng::blocks::Block;
use pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Hex formatting helpers
// ---------------------------------------------------------------------------

fn hb(b: u8)   -> String { format!("{:#04x}", b) }
fn hw(v: u16)  -> String { format!("{:#06x}", v) }
fn hd(v: u32)  -> String { format!("{:#010x}", v) }
fn hq(v: u64)  -> String { format!("{:#018x}", v) }
fn hbs(v: &[u8]) -> String {
    v.iter().map(|b| format!("{:#04x}", b)).collect::<Vec<_>>().join(", ")
}

// ---------------------------------------------------------------------------
// URB types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct UrbHeader {
    id:             String,
    typ:            UrbType,
    #[serde(skip)]
    transfer_type:  TransferType,
    endpoint:       UsbEndpoint,
    #[serde(skip)]
    device_address: String,
    #[serde(skip)]
    bus_number:     String,
    #[serde(skip)]
    urb_status:     UrbStatus,
    #[serde(skip)]
    transfer_flags: String,
    #[serde(skip)]
    setup_fragment: Option<SetupFragment>,
    data_fragment:  Option<DataFragment>,
}

#[derive(Debug, Serialize)]
enum CommDirection { HostIn, HostOut }

#[derive(Debug, Serialize)]
enum UrbType { Submit, Complete, UrbError }

#[derive(Debug, Serialize)]
enum TransferType { Isochronous, Interrupt, Control, Bulk }

struct UsbEndpoint {
    raw:       String,
    direction: CommDirection,
}

impl serde::Serialize for UsbEndpoint {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.direction.serialize(s)
    }
}

impl std::fmt::Debug for UsbEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}({})", self.direction, self.raw)
    }
}

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct SetupFragment {
    bmRequestType: String,
    bRequest:      String,
    wValue:        String,
    wIndex:        String,
    wLength:       String,
}

#[derive(Debug, Serialize)]
enum UrbStatus {
    Success,
    Err(String),
}

// ---------------------------------------------------------------------------
// SB Protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
enum DataFragment {
    Other(String),
    SbProtocol(SbCommand),
}

/// Decoded SB HID command. Frame: 5a [CMD] [LEN] [PAYLOAD...]
/// Variants with no fields are unit/request forms; response forms carry data.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum SbCommand {
    // 0x02
    DeviceAck                  { echoed_cmd: String },
    // 0x05
    DeviceIdentifyRequest,
    DeviceIdentifyResponse     { flags: String },
    // 0x06
    Ping,
    // 0x07
    GetFirmwareStringRequest   { typ: String },
    GetFirmwareStringResponse  { firmware: String },
    // 0x10
    GetSerialRequest,
    GetSerialResponse          { serial: String },
    // 0x11
    StatusRequest              { family: String, feature_id: String },
    StatusResponse             { features: Vec<FeatureEntry> },
    // 0x12
    WriteSingleFeature         { feature: FeatureEntry },
    // 0x15
    BulkRangeDumpRequest,
    BulkRangeDump              { count: u8, entries: Vec<BulkRangeEntry> },
    // 0x20
    GetHardwareIdRequest,
    GetHardwareIdResponse      { hw_id: String },
    // 0x26
    GlobalProfileRequest,
    GlobalProfileResponse      { sbx_master: bool, scout_mode: bool, eq_enable: bool },
    GlobalProfileWrite         { feature: String, enabled: bool },
    // 0x2c
    OutputSelectReadRequest,
    OutputSelectReadResponse   { output: String },
    OutputSelectWrite          { output: String },
    OutputSelectEnumerate      { raw: String },
    // 0x30
    GetDspVersionRequest,
    GetDspVersionResponse      { version: String },
    // 0x39 (Appendix A)
    DirectModeSet              { enabled: bool },
    DirectModeCommit,
    DirectModeReadRequest,
    DirectModeReadResponse     { raw: String },
    DirectModeUnsupported      { sub: String },
    // 0x3a
    Capabilities               { sub: String },
    // 0x3c
    GainConfigRequest,
    GainConfigResponse         { gain: String },
    // 0x6c
    DacFilterReadRequest,
    DacFilterReadResponse      { filter: String },
    DacFilterWrite             { filter: String },
    DacFilterEnumerateRequest,
    DacFilterEnumerateResponse { raw: String },
    // 0x6e
    Notification               { sub: String },
    // fallback
    Unknown                    { cmd: String, raw: String },
}

#[derive(Debug, Serialize)]
struct FeatureEntry {
    family: String,
    name:   String,
    value:  Option<f32>,
}

#[derive(Debug, Serialize)]
struct BulkRangeEntry {
    family: String,
    id:     String,
    max:    f32,
    min:    f32,
    step:   f32,
}

// ---------------------------------------------------------------------------
// URB header parser
// ---------------------------------------------------------------------------

fn parse_packet_urb(packet: &EnhancedPacketBlock) -> UrbHeader {
    let urb_id = u64::from_le_bytes(packet.data[0..8].try_into().unwrap());

    let typ = match packet.data[8] {
        0x53 => UrbType::Submit,
        0x43 => UrbType::Complete,
        _    => UrbType::UrbError,
    };

    let transfer_type = match packet.data[9] {
        0x00 => TransferType::Isochronous,
        0x01 => TransferType::Interrupt,
        0x02 => TransferType::Control,
        0x03 => TransferType::Bulk,
        _    => TransferType::Bulk,
    };

    let endpoint = UsbEndpoint {
        raw:       hb(packet.data[10]),
        direction: if packet.data[10] & 0x80 != 0 {
            CommDirection::HostIn
        } else {
            CommDirection::HostOut
        },
    };

    let bus_number = u16::from_le_bytes(packet.data[12..14].try_into().unwrap());

    let setup_fragment = match packet.data[14] {
        0x00 => Some(SetupFragment {
            bmRequestType: hb(packet.data[40]),
            bRequest:      hb(packet.data[41]),
            wValue: hw(u16::from_le_bytes(packet.data[42..44].try_into().unwrap())),
            wIndex: hw(u16::from_le_bytes(packet.data[44..46].try_into().unwrap())),
            wLength: hw(u16::from_le_bytes(packet.data[46..48].try_into().unwrap())),
        }),
        _ => None,
    };

    let data_fragment = Some(parse_data_fragment(packet));

    let urb_status_value = i32::from_le_bytes(packet.data[28..32].try_into().unwrap());
    let urb_status = match urb_status_value {
        0 => UrbStatus::Success,
        e => UrbStatus::Err(hd(e as u32)),
    };

    let transfer_flags = u32::from_le_bytes(packet.data[56..60].try_into().unwrap());

    UrbHeader {
        id:             hq(urb_id),
        typ,
        transfer_type,
        endpoint,
        device_address: hb(packet.data[11]),
        bus_number:     hw(bus_number),
        urb_status,
        transfer_flags: hd(transfer_flags),
        setup_fragment,
        data_fragment,
    }
}

fn parse_data_fragment(packet: &EnhancedPacketBlock) -> DataFragment {
    let d: &[u8] = &packet.data[64..];
    if d.len() < 3 || d[0] != 0x5a {
        return DataFragment::Other(hbs(d));
    }
    DataFragment::SbProtocol(decode_sb(d[1], d[2] as usize, d))
}

// ---------------------------------------------------------------------------
// Command decoder
// d[0]=0x5a  d[1]=cmd  d[2]=len  d[3..]=payload
// ---------------------------------------------------------------------------

fn decode_sb(cmd: u8, len: usize, d: &[u8]) -> SbCommand {
    let g = |i: usize| d.get(i).copied().unwrap_or(0);
    let tail = |from: usize| hbs(&d[from.min(d.len())..]);

    match cmd {
        // DeviceAck: 5a 02 0a [echoed_cmd] 00 [payload_echo...]
        0x02 => SbCommand::DeviceAck { echoed_cmd: hb(g(3)) },

        // DeviceIdentify
        0x05 if len == 0 => SbCommand::DeviceIdentifyRequest,
        0x05             => SbCommand::DeviceIdentifyResponse { flags: hb(g(3)) },

        // Ping: 5a 06 01 01
        0x06 => SbCommand::Ping,

        // GetFirmwareString
        0x07 if len == 1 => SbCommand::GetFirmwareStringRequest { typ: hb(g(3)) },
        0x07 => {
            let end = (3 + len).min(d.len());
            SbCommand::GetFirmwareStringResponse {
                firmware: String::from_utf8_lossy(&d[3..end]).into_owned(),
            }
        }

        // GetSerial
        0x10 if len == 0 => SbCommand::GetSerialRequest,
        0x10 => {
            let end = (3 + len).min(d.len());
            SbCommand::GetSerialResponse { serial: hbs(&d[3..end]) }
        }

        // Status: LEN=3 → request; LEN≥8 → response (count at d[3], features at d[5])
        0x11 if len == 3 => SbCommand::StatusRequest {
            family:     hb(g(4)),
            feature_id: hb(g(5)),
        },
        0x11 => {
            let count = g(3) as usize;
            let mut features = Vec::with_capacity(count);
            let mut off = 5;
            for _ in 0..count {
                if off + 6 <= d.len() {
                    features.push(feature_at(d, off));
                    off += 6;
                }
            }
            SbCommand::StatusResponse { features }
        }

        // WriteSingleFeature: 5a 12 07 01 [family] [feature_id] 00 [f32_LE×4]
        0x12 if len == 7 => {
            let feature = if d.len() >= 11 {
                feature_raw(g(4), g(5), [g(7), g(8), g(9), g(10)])
            } else {
                FeatureEntry { family: "?".into(), name: "?".into(), value: None }
            };
            SbCommand::WriteSingleFeature { feature }
        }

        // BulkRangeDump: 5a 15 [len] [unknown] [count] [entries...]
        // Each entry: family(1) id(1) max_f32(4) min_f32(4) step_f32(4) = 14 bytes
        0x15 if len == 1 => SbCommand::BulkRangeDumpRequest,
        0x15 => {
            let count = g(4) as usize;
            let mut entries = Vec::with_capacity(count);
            let mut off = 5;
            for _ in 0..count {
                if off + 14 <= d.len() {
                    let f32_at = |i: usize| {
                        f32::from_le_bytes(d[i..i+4].try_into().unwrap())
                    };
                    entries.push(BulkRangeEntry {
                        family: hb(d[off]),
                        id:     hb(d[off + 1]),
                        max:    f32_at(off + 2),
                        min:    f32_at(off + 6),
                        step:   f32_at(off + 10),
                    });
                    off += 14;
                }
            }
            SbCommand::BulkRangeDump { count: count as u8, entries }
        }

        // GetHardwareId
        0x20 if len == 0 => SbCommand::GetHardwareIdRequest,
        0x20             => SbCommand::GetHardwareIdResponse { hw_id: hb(g(3)) },

        // GlobalProfile: bitmask — bit0=SBX Master, bit1=Scout Mode, bit2=EQ Enable
        // Send query: 5a 26 03 08 ff ff
        // Response:   5a 26 0b 08 ff ff [bitmask] 00...
        // Write:      5a 26 05 07 [feature_id] 00 [state] 00
        0x26 => match (g(3), len) {
            (0x08, 3)  => SbCommand::GlobalProfileRequest,
            (0x08, _)  => {
                let bitmask = g(6);
                SbCommand::GlobalProfileResponse {
                    sbx_master: bitmask & 0x01 != 0,
                    scout_mode: bitmask & 0x02 != 0,
                    eq_enable:  bitmask & 0x04 != 0,
                }
            }
            (0x07, _) => {
                let feature = match g(4) {
                    0x01 => "SBX Master",
                    0x02 => "Scout Mode",
                    _    => "Unknown",
                };
                SbCommand::GlobalProfileWrite { feature: feature.into(), enabled: g(6) != 0 }
            }
            _ => SbCommand::Unknown { cmd: hb(0x26), raw: tail(3) },
        },

        // OutputSelect: d[3] = direction (0x00=write, 0x01=read, 0x02=enumerate)
        0x2c => {
            let output_str = |mode: u8| match mode {
                0x02 => "Speakers".into(),
                0x04 => "Headphones".into(),
                m    => hb(m),
            };
            match (g(3), len) {
                (0x01, 1) => SbCommand::OutputSelectReadRequest,
                (0x01, _) => SbCommand::OutputSelectReadResponse { output: output_str(g(4)) },
                (0x00, _) => SbCommand::OutputSelectWrite       { output: output_str(g(4)) },
                (0x02, _) => SbCommand::OutputSelectEnumerate   { raw: tail(4) },
                _         => SbCommand::Unknown { cmd: hb(0x2c), raw: tail(3) },
            }
        }

        // GetDspVersion
        0x30 if len == 0 => SbCommand::GetDspVersionRequest,
        0x30 => {
            let end = (3 + len).min(d.len());
            SbCommand::GetDspVersionResponse { version: hbs(&d[3..end]) }
        }

        // DirectMode (Appendix A)
        // Set:    5a 39 03 00 05 [01|00]
        // Commit: 5a 39 01 01
        // Read:   5a 39 01 02  →  5a 39 [len] 02 [data...]
        // Subs 0x04/0x05 return 0x81 ACK (not supported on G6)
        0x39 => match (len, g(3)) {
            (3, 0x00)           => SbCommand::DirectModeSet { enabled: g(5) == 0x01 },
            (l, 0x01) if l <= 2 => SbCommand::DirectModeCommit,
            (1, 0x02)           => SbCommand::DirectModeReadRequest,
            (_, 0x02)           => SbCommand::DirectModeReadResponse {
                raw: { let end = (3 + len).min(d.len()); hbs(&d[3..end]) }
            },
            (_, s @ 0x04) | (_, s @ 0x05) => SbCommand::DirectModeUnsupported { sub: hb(s) },
            _                   => SbCommand::Unknown { cmd: hb(0x39), raw: tail(3) },
        },

        // Capabilities / RGB lighting — sub-command at d[3]
        0x3a => SbCommand::Capabilities { sub: hb(g(3)) },

        // GainConfig: Send 5a 3c 02 01 00 → Recv 5a 3c 04 01 00 [gain] 00
        // Both have d[3]=0x01; request has LEN=2, response has LEN=4 with gain at d[5]
        0x3c => match len {
            2 => SbCommand::GainConfigRequest,
            4 => SbCommand::GainConfigResponse { gain: hb(g(5)) },
            _ => SbCommand::Unknown { cmd: hb(0x3c), raw: tail(3) },
        },

        // DacFilter: d[3] = direction (0x00=write, 0x01=read, 0x02=enumerate)
        0x6c => {
            let filter_str = |f: u8| match f {
                0x01 => "Fast Roll-off, Minimum Phase".into(),
                0x02 => "Slow Roll-off, Minimum Phase".into(),
                0x03 => "NOS (Non-Oversampling)".into(),
                0x04 => "Fast Roll-off, Linear Phase".into(),
                0x05 => "Slow Roll-off, Linear Phase".into(),
                f    => hb(f),
            };
            match (g(3), len) {
                (0x01, 1) => SbCommand::DacFilterReadRequest,
                (0x01, _) => SbCommand::DacFilterReadResponse  { filter: filter_str(g(4)) },
                (0x00, _) => SbCommand::DacFilterWrite         { filter: filter_str(g(4)) },
                (0x02, 1) => SbCommand::DacFilterEnumerateRequest,
                (0x02, _) => SbCommand::DacFilterEnumerateResponse { raw: tail(4) },
                _         => SbCommand::Unknown { cmd: hb(0x6c), raw: tail(3) },
            }
        }

        // Notification
        0x6e => SbCommand::Notification { sub: hb(g(3)) },

        _ => SbCommand::Unknown { cmd: hb(cmd), raw: tail(0) },
    }
}

// ---------------------------------------------------------------------------
// Feature helpers
// ---------------------------------------------------------------------------

/// Parse one feature entry from d[off..off+6]: [family, id, f32_LE×4]
fn feature_at(d: &[u8], off: usize) -> FeatureEntry {
    if off + 6 > d.len() {
        return FeatureEntry { family: "?".into(), name: "?".into(), value: None };
    }
    feature_raw(d[off], d[off + 1], [d[off+2], d[off+3], d[off+4], d[off+5]])
}

fn feature_raw(family: u8, id: u8, f32_bytes: [u8; 4]) -> FeatureEntry {
    let value = Some(f32::from_le_bytes(f32_bytes));
    let name = match family {
        0x96 => dsp_feature_name(id),
        _    => format!("id_{}", hb(id)),
    };
    FeatureEntry { family: hb(family), name, value }
}

fn dsp_feature_name(id: u8) -> String {
    match id {
        0x00 => "SurroundToggle",
        0x01 => "SurroundLevel",
        0x02 => "DialogPlusToggle",
        0x03 => "DialogPlusLevel",
        0x04 => "SmartVolToggle",
        0x05 => "SmartVolLevel",
        0x06 => "SmartVolMode",
        0x07 => "CrystalizerToggle",
        0x08 => "CrystalizerLevel",
        0x09 => "EqToggle",
        0x0a => "EqPreAmp",
        0x0b => "Eq31Hz",
        0x0c => "Eq62Hz",
        0x0d => "Eq125Hz",
        0x0e => "Eq250Hz",
        0x0f => "Eq500Hz",
        0x10 => "Eq1kHz",
        0x11 => "Eq2kHz",
        0x12 => "Eq4kHz",
        0x13 => "Eq8kHz",
        0x14 => "Eq16kHz",
        0x17 => "SurroundDistance",
        0x18 => "BassToggle",
        0x19 => "BassLevel",
        _    => return format!("Unknown_{}", hb(id)),
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn parse_file(path: &str) -> Vec<UrbHeader> {
    let file = File::open(path)
        .unwrap_or_else(|e| panic!("Cannot open {path}: {e}"));
    let mut reader = PcapNgReader::new(file).unwrap();
    let mut packets = Vec::new();
    while let Some(block) = reader.next_block() {
        let block = block.unwrap();
        if let Block::EnhancedPacket(packet) = block {
            if packet.data.get(64) == Some(&0x5a) {
                packets.push(parse_packet_urb(&packet));
            }
        }
    }
    packets
}

fn write_parsed(path: &str) {
    let input = std::path::Path::new(path);
    let stem = input.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned());
    let out_path = format!("parsed/{stem}.json");

    eprint!("{path} ... ");
    let packets = parse_file(path);
    fs::create_dir_all("parsed").expect("Cannot create ./parsed");
    let out = File::create(&out_path)
        .unwrap_or_else(|e| panic!("Cannot create {out_path}: {e}"));
    serde_json::to_writer_pretty(BufWriter::new(out), &packets)
        .expect("JSON serialization failed");
    eprintln!("wrote {} packets → {out_path}", packets.len());
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--all") | Some("-a") => {
            let mut files: Vec<String> = fs::read_dir("captures")
                .expect("Cannot read ./captures")
                .filter_map(|e| {
                    let e = e.ok()?;
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("pcapng") {
                        Some(p.to_string_lossy().into_owned())
                    } else {
                        None
                    }
                })
                .collect();
            files.sort();
            for path in &files {
                write_parsed(path);
            }
        }
        Some(path) => write_parsed(path),
        None => {
            eprintln!("Usage:");
            eprintln!("  cargo run -- <file.pcapng>   parse a specific capture");
            eprintln!("  cargo run -- --all / -a      parse all captures in ./captures");
            std::process::exit(1);
        }
    }
}
