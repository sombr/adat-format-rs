import zlib
import struct

TOC_SIZE = 128 + 4*4

test_string = """
1 hello world from a test file!
2 hello world from a test file!
3 hello world from a test file!
4 hello world from a test file!
5 hello world from a test file!
""".encode(encoding="ascii")
compressed = zlib.compress(test_string, level = 9)

print(len(test_string), len(compressed))

with open("TEST.dat", "wb") as file:
    # header
    file.write(b"ADAT") # magic
    file.write(struct.pack("<L", 4*4)) # offset
    file.write(struct.pack("<L", TOC_SIZE)) # length / just one
    file.write(struct.pack("<L", 9)) # version = 9

    # entry
    file.write(struct.pack("128s", b"some/path/foo.txt"))
    file.write(struct.pack("<L", 4*4 + TOC_SIZE)) # offset
    file.write(struct.pack("<L", len(test_string))) # decompressed length
    file.write(struct.pack("<L", len(compressed))) # compressed length
    file.write(struct.pack("<L", 0)) # ???

    # data
    file.write(compressed)
