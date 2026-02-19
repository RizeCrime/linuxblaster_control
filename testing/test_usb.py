import usb.core
import usb.util
import struct
import time

dev = usb.core.find(idVendor=0x041e, idProduct=0x3256)
if dev.is_kernel_driver_active(4):
    dev.detach_kernel_driver(4)

print("Resetting device...")
dev.reset()
time.sleep(1)

dev = usb.core.find(idVendor=0x041e, idProduct=0x3256)
if dev.is_kernel_driver_active(4):
    dev.detach_kernel_driver(4)
usb.util.claim_interface(dev, 4)

def send(data):
    dev.ctrl_transfer(0x21, 0x09, 0x200, 0x04, bytes(data) + bytes(64 - len(data)))

def recv(timeout=1000):
    try:
        return dev.read(0x85, 64, timeout=timeout).tobytes()
    except usb.core.USBTimeoutError:
        return None

def recv_all(timeout=500):
    """Drain all pending responses"""
    responses = []
    while True:
        r = recv(timeout)
        if r is None:
            break
        responses.append(r)
    return responses

def send_recv(data, label=""):
    send(data)
    r = recv()
    if r:
        print(f"  {label:25s} -> {r[:12].hex(' ')}")
    else:
        print(f"  {label:25s} -> TIMEOUT")
    return r

def f32_val(resp, offset=7):
    return struct.unpack('<f', resp[offset:offset+4])[0]

# === Init ===
print("=== Init ===")
send_recv([0x5a, 0x05, 0x00],                    "DeviceIdentify")
send_recv([0x5a, 0x10, 0x00],                    "GetSerial")
send_recv([0x5a, 0x20, 0x00],                    "GetHardwareId")
send_recv([0x5a, 0x30, 0x00],                    "GetDspVersion")
send_recv([0x5a, 0x06, 0x01, 0x01],              "Ping")

r = send_recv([0x5a, 0x07, 0x01, 0x02],          "GetFirmware")
if r:
    fw = bytes(r[3:3+r[2]]).decode('ascii', errors='replace')
    print(f"  {'':25s}    FW: \"{fw}\"")

# === Read all DSP features ===
print("\n=== Family 0x96 (DSP) ===")
names_96 = {
    0x00: "SbxUnknown0",   0x01: "SurroundLevel",
    0x02: "Dialog+Toggle", 0x03: "Dialog+Level",
    0x04: "SmartVolToggle", 0x05: "SmartVolLevel",
    0x06: "BassToggle",    0x07: "CrystalizerToggle",
    0x08: "CrystalizerLvl", 0x09: "EqToggle",
    0x0a: "EqPreAmp",      0x0b: "Eq31Hz",
    0x0c: "Eq62Hz",        0x0d: "Eq125Hz",
    0x0e: "Eq250Hz",       0x0f: "Eq500Hz",
    0x10: "Eq1kHz",        0x11: "Eq2kHz",
    0x12: "Eq4kHz",        0x13: "Eq8kHz",
    0x14: "Eq16kHz",       0x17: "SurroundDist",
}

for fid in sorted(names_96.keys()):
    r = send_recv([0x5a, 0x11, 0x03, 0x01, 0x96, fid], names_96[fid])
    if r:
        print(f"  {'':25s}    = {f32_val(r)}")

# === Test a write ===
print("\n=== Toggle Crystalizer ON ===")
send([0x5a, 0x12, 0x07, 0x01, 0x96, 0x07, 0x00, 0x00, 0x80, 0x3f])
for r in recv_all():
    print(f"  <- {r[:12].hex(' ')}")

print("\nDone!")
usb.util.release_interface(dev, 4)
