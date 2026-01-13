# SoundBlasterX G6 USB Protocol

- All Commands must be 64 Bytes long (65 with HID Report ID prefix)

## Overview

The device uses multiple command formats with hierarchical dependencies:
- **Format 2 (0x26)**: Master/system-level controls that gate other features
- **Format 1 (0x1207/0x1103)**: Audio processing features (requires SBX enabled via Format 2)
- **Format 3 (0x3a)**: RGB lighting control (independent)
(Naming was done in the order I found them, the numbers don't mean anything special; Call them Alice, Bob, and Chungus if you want) 

---

## Format 2: Master Controls (0x26 Family)

**Purpose:** System-level toggles that control whether other features can be used.

**Pattern:** SET + COMMIT (symmetric for ON/OFF)

### Command Structure

**SET Command:**
| Byte | 0    | 1    | 2    | 3    | 4          | 5    | 6     | 7+    |
| ---- | ---- | ---- | ---- | ---- | ---------- | ---- | ----- | ----- |
| Value| 0x5a | 0x26 | 0x05 | 0x07 | Feature ID | 0x00 | State | zeros |

**COMMIT Command (identical for all 0x26 features):**
| Byte | 0    | 1    | 2    | 3    | 4-5    | 6+    |
| ---- | ---- | ---- | ---- | ---- | ------ | ----- |
| Value| 0x5a | 0x26 | 0x03 | 0x08 | 0xffff | zeros |

### Known Features

| Feature ID | Feature Name | Purpose | State Values |
| ---------- | ------------ | ------- | ------------ |
| 0x01       | SBX          | Master toggle for all Format 1 audio features | 0x00=OFF, 0x01=ON |
| 0x02       | Scout Mode   | Gaming audio enhancement (footsteps, etc.) | 0x00=OFF, 0x01=ON |

### Notes

- **Dependency:** Format 1 audio features require SBX (0x01) to be enabled first
- Constants 0x07/0x08 at bytes 3 in SET/COMMIT match Crystalizer IDs from Format 1
- COMMIT command is completely feature-agnostic (same bytes for all features)
- The 0xffff value (bytes 4-5 of COMMIT) appears to be a constant magic value

---

## Format 1: Audio Processing Features (0x1207/0x1103 Family)

**Purpose:** Individual audio effect controls (Surround, EQ, Bass, etc.)

**Requirement:** SBX must be enabled (Format 2, Feature 0x01) for these features to function.

**Pattern:** DATA + COMMIT

### Command Structure

| Byte | 0    | 1    | 2    | 3    | 4    | 5          | 6-9         | 10+   |
| ---- | ---- | ---- | ---- | ---- | ---- | ---------- | ----------- | ----- |
| DATA | 0x5a | 0x12 | 0x07 | 0x01 | 0x96 | Feature ID | Value Bytes | zeros |
| COMMIT| 0x5a| 0x11 | 0x03 | 0x01 | 0x96 | Feature ID | 0x00000000  | zeros |

### Command Types

| Type   | Value  | Purpose |
| ------ | ------ | ------- |
| DATA   | 0x1207 | Set feature value |
| COMMIT | 0x1103 | Apply the change |

### Feature IDs

| ID   | Type   | Feature      | Value Type                    |
| ---- | ------ | ------------ | ----------------------------- |
| 0x00 | Toggle | Surround     | 0x803f (on), 0x0000 (off)     |
| 0x01 | Slider | Surround     | Normalized float (0.0-1.0)    |
| 0x02 | Toggle | Dialog+      | 0x803f (on), 0x0000 (off)     |
| 0x03 | Slider | Dialog+      | Normalized float (0.0-1.0)    |
| 0x04 | Toggle | SmartVolume  | 0x803f (on), 0x0000 (off)     |
| 0x05 | Slider | SmartVolume  | Normalized float (0.0-1.0)    |
| 0x06 | Slider | SmartVolume  | Special: 0x0040 (Night), 0x803f (Loud) |
| 0x07 | Toggle | Crystalizer  | 0x803f (on), 0x0000 (off)     |
| 0x08 | Slider | Crystalizer  | Normalized float (0.0-1.0)    |
| 0x09 | Toggle | EQ           | 0x803f (on), 0x0000 (off)     |
| 0x0a | Slider | EQ Pre-Amp   | Float dB (-12dB to +12dB)     |
| 0x0b | Slider | EQ 31Hz      | Float dB (-12dB to +12dB)     |
| 0x0c | Slider | EQ 62Hz      | Float dB (-12dB to +12dB)     |
| 0x0d | Slider | EQ 125Hz     | Float dB (-12dB to +12dB)     |
| 0x0e | Slider | EQ 250Hz     | Float dB (-12dB to +12dB)     |
| 0x0f | Slider | EQ 500Hz     | Float dB (-12dB to +12dB)     |
| 0x10 | Slider | EQ 1kHz      | Float dB (-12dB to +12dB)     |
| 0x11 | Slider | EQ 2kHz      | Float dB (-12dB to +12dB)     |
| 0x12 | Slider | EQ 4kHz      | Float dB (-12dB to +12dB)     |
| 0x13 | Slider | EQ 8kHz      | Float dB (-12dB to +12dB)     |
| 0x14 | Slider | EQ 16kHz     | Float dB (-12dB to +12dB)     |
| 0x18 | Toggle | Bass         | 0x803f (on), 0x0000 (off)     |
| 0x19 | Slider | Bass         | Normalized float (0.0-1.0)    |

### Notes

- Toggle values: `0x803f` (ON) and `0x0000` (OFF) are stored as little-endian floats (1.0 and 0.0)
- Slider features have IDs that are toggle_id + 1
- EQ bands use raw dB values as IEEE 754 floats, unlike other sliders (normalized 0-1)
- Magic number `0x0196` appears in all Format 1 commands

---

## Format 3: RGB Lighting Control (0x3a Family)

**Purpose:** RGB lighting toggle and configuration

**Pattern:** Asymmetric - OFF uses 1 command, ON uses 3 commands

### RGB OFF (Single Command)

| Byte | 0    | 1    | 2    | 3    | 4    | 5+    |
| ---- | ---- | ---- | ---- | ---- | ---- | ----- |
| Value| 0x5a | 0x3a | 0x02 | 0x06 | 0x00 | zeros |

### RGB ON (Three Commands)

**Command 1 - Enable:**
| Byte | 0    | 1    | 2    | 3    | 4    | 5+    |
| ---- | ---- | ---- | ---- | ---- | ---- | ----- |
| Value| 0x5a | 0x3a | 0x02 | 0x06 | 0x01 | zeros |

**Command 2 - Configuration (Mode?):**
| Byte | 0    | 1    | 2    | 3+    |
| ---- | ---- | ---- | ---- | ----- |
| Value| 0x5a | 0x3a | 0x06 | 04 00 03 01 00 01 ... |

**Command 3 - Configuration (Color Data?):**
| Byte | 0    | 1    | 2    | 3+    |
| ---- | ---- | ---- | ---- | ----- |
| Value| 0x5a | 0x3a | 0x09 | 0a 00 03 01 01 ff 00 00 ff ... |

### Notes

- Asymmetric pattern: OFF requires only 1 command, ON requires 3
- Commands 2 & 3 only sent when enabling (may set mode, pattern, or default colors)
- Bytes `ff 00 00 ff` in Command 3 could represent RGBA color values
- No COMMIT pattern; commands appear to execute immediately
- Independent of other formats (no dependency on SBX or other features)

---

## Feature Dependencies

```
Format 2 (0x26) - Master Controls
├── SBX (0x01) ────────────┐
│   └── [Enables all Format 1 features]
│                           │
└── Scout Mode (0x02)       │
                            ▼
                   Format 1 (0x1207/0x1103) - Audio Features
                   ├── Surround (0x00/0x01)
                   ├── Dialog+ (0x02/0x03)
                   ├── SmartVolume (0x04/0x05/0x06)
                   ├── Crystalizer (0x07/0x08)
                   ├── EQ (0x09-0x14)
                   └── Bass (0x18/0x19)

Format 3 (0x3a) - RGB Lighting
└── RGB Control (independent)
```

**Key Point:** Format 1 audio features can only be toggled when SBX (Format 2, Feature 0x01) is enabled.
