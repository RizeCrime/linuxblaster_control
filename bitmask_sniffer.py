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
        print(f"  {label:20s} -> {r[:12].hex(' ')}")
    else:
        print(f"  {label:20s} -> TIMEOUT")
    return r

def read_sbx():
    r = send_recv([0x5a, 0x26, 0x03, 0x08, 0xff, 0xff], "SBX bitmask")
    if r:
        val = r[6]
        print(f"  {'':20s}    bitmask: 0x{val:02x} = 0b{val:08b}")
    return r

# Init
print("=== Init ===")
send_recv([0x5a, 0x05, 0x00], "DeviceIdentify")
send_recv([0x5a, 0x06, 0x01, 0x01], "Ping")

print("\n=== Current state ===")
read_sbx()

print("\n=== Toggle SBX OFF ===")
send([0x5a, 0x26, 0x05, 0x07, 0x01, 0x00, 0x00, 0x00])
recv_all()
read_sbx()

print("\n=== Toggle SBX ON ===")
send([0x5a, 0x26, 0x05, 0x07, 0x01, 0x00, 0x01, 0x00])
recv_all()
read_sbx()

# Toggle individual features
for name, fid, val in [
    ("Crystalizer ON",  0x07, 1.0),
    ("Crystalizer OFF", 0x07, 0.0),
    ("SmartVol OFF",    0x04, 0.0),
    ("SmartVol ON",     0x04, 1.0),
    ("Dialog+ ON",      0x02, 1.0),
    ("Dialog+ OFF",     0x02, 0.0),
    ("Bass ON",         0x06, 1.0),
    ("Bass OFF",        0x06, 0.0),
    ("Surround ON",     0x01, 1.0),
    ("Surround OFF",    0x01, 0.0),
]:
    cmd = [0x5a, 0x12, 0x07, 0x01, 0x96, fid, 0x00] + list(struct.pack('<f', val))
    send(cmd)
    recv_all()
    print(f"\nAfter {name}:")
    read_sbx()

usb.util.release_interface(dev, 4)
print("\nDone!")

print("Current:")
read_sbx()

print("\nEQ OFF:")
send([0x5a, 0x12, 0x07, 0x01, 0x96, 0x09, 0x00, 0x00, 0x00, 0x00])
recv_all()
read_sbx()

print("\nEQ ON:")
send([0x5a, 0x12, 0x07, 0x01, 0x96, 0x09, 0x00, 0x00, 0x80, 0x3f])
recv_all()
read_sbx()

print("Current:")
read_sbx()

print("\nScout Mode ON:")
send([0x5a, 0x26, 0x05, 0x07, 0x02, 0x00, 0x01, 0x00])
recv_all()
read_sbx()

print("\nScout Mode OFF:")
send([0x5a, 0x26, 0x05, 0x07, 0x02, 0x00, 0x00, 0x00])
recv_all()
read_sbx()



