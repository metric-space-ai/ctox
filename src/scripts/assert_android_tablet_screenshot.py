"""Check the Android tablet smoke screenshot's landscape orientation and ratio."""

import struct
import sys

with open(sys.argv[1], "rb") as image:
    header = image.read(24)
width, height = struct.unpack(">II", header[16:24])
assert width > height, f"tablet did not rotate to landscape: {width}x{height}"
assert abs((width / height) - (4 / 3)) < 0.08, f"tablet is not 4:3: {width}x{height}"
