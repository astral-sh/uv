import binascii
import sys

PY26 = sys.version_info[0] == 2 and sys.version_info[1] <= 6

if PY26:
    import struct


class PgpdumpException(Exception):
    '''Base exception class raised by any parsing errors, etc.'''
    pass


# 256 values corresponding to each possible byte
CRC24_TABLE = (
    0x000000, 0x864cfb, 0x8ad50d, 0x0c99f6, 0x93e6e1, 0x15aa1a, 0x1933ec,
    0x9f7f17, 0xa18139, 0x27cdc2, 0x2b5434, 0xad18cf, 0x3267d8, 0xb42b23,
    0xb8b2d5, 0x3efe2e, 0xc54e89, 0x430272, 0x4f9b84, 0xc9d77f, 0x56a868,
    0xd0e493, 0xdc7d65, 0x5a319e, 0x64cfb0, 0xe2834b, 0xee1abd, 0x685646,
    0xf72951, 0x7165aa, 0x7dfc5c, 0xfbb0a7, 0x0cd1e9, 0x8a9d12, 0x8604e4,
    0x00481f, 0x9f3708, 0x197bf3, 0x15e205, 0x93aefe, 0xad50d0, 0x2b1c2b,
    0x2785dd, 0xa1c926, 0x3eb631, 0xb8faca, 0xb4633c, 0x322fc7, 0xc99f60,
    0x4fd39b, 0x434a6d, 0xc50696, 0x5a7981, 0xdc357a, 0xd0ac8c, 0x56e077,
    0x681e59, 0xee52a2, 0xe2cb54, 0x6487af, 0xfbf8b8, 0x7db443, 0x712db5,
    0xf7614e, 0x19a3d2, 0x9fef29, 0x9376df, 0x153a24, 0x8a4533, 0x0c09c8,
    0x00903e, 0x86dcc5, 0xb822eb, 0x3e6e10, 0x32f7e6, 0xb4bb1d, 0x2bc40a,
    0xad88f1, 0xa11107, 0x275dfc, 0xdced5b, 0x5aa1a0, 0x563856, 0xd074ad,
    0x4f0bba, 0xc94741, 0xc5deb7, 0x43924c, 0x7d6c62, 0xfb2099, 0xf7b96f,
    0x71f594, 0xee8a83, 0x68c678, 0x645f8e, 0xe21375, 0x15723b, 0x933ec0,
    0x9fa736, 0x19ebcd, 0x8694da, 0x00d821, 0x0c41d7, 0x8a0d2c, 0xb4f302,
    0x32bff9, 0x3e260f, 0xb86af4, 0x2715e3, 0xa15918, 0xadc0ee, 0x2b8c15,
    0xd03cb2, 0x567049, 0x5ae9bf, 0xdca544, 0x43da53, 0xc596a8, 0xc90f5e,
    0x4f43a5, 0x71bd8b, 0xf7f170, 0xfb6886, 0x7d247d, 0xe25b6a, 0x641791,
    0x688e67, 0xeec29c, 0x3347a4, 0xb50b5f, 0xb992a9, 0x3fde52, 0xa0a145,
    0x26edbe, 0x2a7448, 0xac38b3, 0x92c69d, 0x148a66, 0x181390, 0x9e5f6b,
    0x01207c, 0x876c87, 0x8bf571, 0x0db98a, 0xf6092d, 0x7045d6, 0x7cdc20,
    0xfa90db, 0x65efcc, 0xe3a337, 0xef3ac1, 0x69763a, 0x578814, 0xd1c4ef,
    0xdd5d19, 0x5b11e2, 0xc46ef5, 0x42220e, 0x4ebbf8, 0xc8f703, 0x3f964d,
    0xb9dab6, 0xb54340, 0x330fbb, 0xac70ac, 0x2a3c57, 0x26a5a1, 0xa0e95a,
    0x9e1774, 0x185b8f, 0x14c279, 0x928e82, 0x0df195, 0x8bbd6e, 0x872498,
    0x016863, 0xfad8c4, 0x7c943f, 0x700dc9, 0xf64132, 0x693e25, 0xef72de,
    0xe3eb28, 0x65a7d3, 0x5b59fd, 0xdd1506, 0xd18cf0, 0x57c00b, 0xc8bf1c,
    0x4ef3e7, 0x426a11, 0xc426ea, 0x2ae476, 0xaca88d, 0xa0317b, 0x267d80,
    0xb90297, 0x3f4e6c, 0x33d79a, 0xb59b61, 0x8b654f, 0x0d29b4, 0x01b042,
    0x87fcb9, 0x1883ae, 0x9ecf55, 0x9256a3, 0x141a58, 0xefaaff, 0x69e604,
    0x657ff2, 0xe33309, 0x7c4c1e, 0xfa00e5, 0xf69913, 0x70d5e8, 0x4e2bc6,
    0xc8673d, 0xc4fecb, 0x42b230, 0xddcd27, 0x5b81dc, 0x57182a, 0xd154d1,
    0x26359f, 0xa07964, 0xace092, 0x2aac69, 0xb5d37e, 0x339f85, 0x3f0673,
    0xb94a88, 0x87b4a6, 0x01f85d, 0x0d61ab, 0x8b2d50, 0x145247, 0x921ebc,
    0x9e874a, 0x18cbb1, 0xe37b16, 0x6537ed, 0x69ae1b, 0xefe2e0, 0x709df7,
    0xf6d10c, 0xfa48fa, 0x7c0401, 0x42fa2f, 0xc4b6d4, 0xc82f22, 0x4e63d9,
    0xd11cce, 0x575035, 0x5bc9c3, 0xdd8538
)


def crc24(data):
    '''Implementation of the CRC-24 algorithm used by OpenPGP.'''
    # CRC-24-Radix-64
    # x24 + x23 + x18 + x17 + x14 + x11 + x10 + x7 + x6
    #   + x5 + x4 + x3 + x + 1 (OpenPGP)
    # 0x864CFB / 0xDF3261 / 0xC3267D
    crc = 0x00b704ce
    # this saves a bunch of slower global accesses
    crc_table = CRC24_TABLE
    for byte in data:
        tbl_idx = ((crc >> 16) ^ byte) & 0xff
        crc = (crc_table[tbl_idx] ^ (crc << 8)) & 0x00ffffff
    return crc


def get_int2(data, offset):
    '''Pull two bytes from data at offset and return as an integer.'''
    return (data[offset] << 8) + data[offset + 1]


def get_int4(data, offset):
    '''Pull four bytes from data at offset and return as an integer.'''
    return ((data[offset] << 24) + (data[offset + 1] << 16) +
            (data[offset + 2] << 8) + data[offset + 3])


def get_int8(data, offset):
    '''Pull eight bytes from data at offset and return as an integer.'''
    return (get_int4(data, offset) << 32) + get_int4(data, offset + 4)


def get_mpi(data, offset):
    '''Gets a multi-precision integer as per RFC-4880.
    Returns the MPI and the new offset.
    See: http://tools.ietf.org/html/rfc4880#section-3.2'''
    mpi_len = get_int2(data, offset)
    offset += 2
    to_process = (mpi_len + 7) // 8
    mpi = 0
    i = -4
    for i in range(0, to_process - 3, 4):
        mpi <<= 32
        mpi += get_int4(data, offset + i)
    for j in range(i + 4, to_process):
        mpi <<= 8
        mpi += data[offset + j]
    # Python 3.2 and later alternative:
    #mpi = int.from_bytes(data[offset:offset + to_process], byteorder='big')
    offset += to_process
    return mpi, offset


def get_hex_data(data, offset, byte_count):
    '''Pull the given number of bytes from data at offset and return as a
    hex-encoded string.'''
    key_data = data[offset:offset + byte_count]
    if PY26:
        key_data = buffer(key_data)
    key_id = binascii.hexlify(key_data)
    return key_id.upper()


def get_key_id(data, offset):
    '''Pull eight bytes from data at offset and return as a 16-byte hex-encoded
    string.'''
    return get_hex_data(data, offset, 8)


def get_int_bytes(data):
    '''Get the big-endian byte form of an integer or MPI.'''
    hexval = '%X' % data
    new_len = (len(hexval) + 1) // 2 * 2
    hexval = hexval.zfill(new_len)
    return binascii.unhexlify(hexval.encode('ascii'))


def pack_data(data):
    '''Pack iterable of binary data into a bytestring if necessary.'''
    if PY26:
        return struct.pack('%dB' % len(data), *data)
    return data

def same_key(key_a, key_b):
    '''Comparison function for key ID or fingerprint strings, taking into
    account varying length.'''
    if len(key_a) == len(key_b):
        return key_a == key_b
    elif len(key_a) < len(key_b):
        return key_b.endswith(key_a)
    else:
        return key_a.endswith(key_b)
