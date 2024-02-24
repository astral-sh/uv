from datetime import datetime, timedelta
import hashlib
from math import ceil, log
import re

from .utils import (PgpdumpException, get_int2, get_int4, get_mpi,
        get_key_id, get_hex_data, get_int_bytes, pack_data)


class Packet(object):
    '''The base packet object containing various fields pulled from the packet
    header as well as a slice of the packet data.'''
    def __init__(self, raw, name, new, data):
        self.raw = raw
        self.name = name
        self.new = new
        self.length = len(data)
        self.data = data

        # now let subclasses work their magic
        self.parse()

    def parse(self):
        '''Perform any parsing necessary to populate fields on this packet.
        This method is called as the last step in __init__(). The base class
        method is a no-op; subclasses should use this as required.'''
        return 0

    def __repr__(self):
        new = "old"
        if self.new:
            new = "new"
        return "<%s: %s (%d), %s, length %d>" % (
                self.__class__.__name__, self.name, self.raw, new, self.length)


class AlgoLookup(object):
    '''Mixin class containing algorithm lookup methods.'''
    pub_algorithms = {
        1:  "RSA Encrypt or Sign",
        2:  "RSA Encrypt-Only",
        3:  "RSA Sign-Only",
        16: "ElGamal Encrypt-Only",
        17: "DSA Digital Signature Algorithm",
        18: "Elliptic Curve",
        19: "ECDSA",
        20: "Formerly ElGamal Encrypt or Sign",
        21: "Diffie-Hellman",
    }

    @classmethod
    def lookup_pub_algorithm(cls, alg):
        if 100 <= alg <= 110:
            return "Private/Experimental algorithm"
        return cls.pub_algorithms.get(alg, "Unknown")

    hash_algorithms = {
        1:  "MD5",
        2:  "SHA1",
        3:  "RIPEMD160",
        8:  "SHA256",
        9:  "SHA384",
        10: "SHA512",
        11: "SHA224",
    }

    @classmethod
    def lookup_hash_algorithm(cls, alg):
        # reserved values check
        if alg in (4, 5, 6, 7):
            return "Reserved"
        if 100 <= alg <= 110:
            return "Private/Experimental algorithm"
        return cls.hash_algorithms.get(alg, "Unknown")

    sym_algorithms = {
        # (Name, IV length)
        0:  ("Plaintext or unencrypted", 0),
        1:  ("IDEA", 8),
        2:  ("Triple-DES", 8),
        3:  ("CAST5", 8),
        4:  ("Blowfish", 8),
        5:  ("Reserved", 8),
        6:  ("Reserved", 8),
        7:  ("AES with 128-bit key", 16),
        8:  ("AES with 192-bit key", 16),
        9:  ("AES with 256-bit key", 16),
        10: ("Twofish with 256-bit key", 16),
        11: ("Camellia with 128-bit key", 16),
        12: ("Camellia with 192-bit key", 16),
        13: ("Camellia with 256-bit key", 16),
    }

    @classmethod
    def _lookup_sym_algorithm(cls, alg):
        return cls.sym_algorithms.get(alg, ("Unknown", 0))

    @classmethod
    def lookup_sym_algorithm(cls, alg):
        return cls._lookup_sym_algorithm(alg)[0]

    @classmethod
    def lookup_sym_algorithm_iv(cls, alg):
        return cls._lookup_sym_algorithm(alg)[1]


class SignatureSubpacket(object):
    '''A signature subpacket containing a type, type name, some flags, and the
    contained data.'''
    CRITICAL_BIT = 0x80
    CRITICAL_MASK = 0x7f

    def __init__(self, raw, hashed, data):
        self.raw = raw
        self.subtype = raw & self.CRITICAL_MASK
        self.hashed = hashed
        self.critical = bool(raw & self.CRITICAL_BIT)
        self.length = len(data)
        self.data = data

    subpacket_types = {
        2:  "Signature Creation Time",
        3:  "Signature Expiration Time",
        4:  "Exportable Certification",
        5:  "Trust Signature",
        6:  "Regular Expression",
        7:  "Revocable",
        9:  "Key Expiration Time",
        10: "Placeholder for backward compatibility",
        11: "Preferred Symmetric Algorithms",
        12: "Revocation Key",
        16: "Issuer",
        20: "Notation Data",
        21: "Preferred Hash Algorithms",
        22: "Preferred Compression Algorithms",
        23: "Key Server Preferences",
        24: "Preferred Key Server",
        25: "Primary User ID",
        26: "Policy URI",
        27: "Key Flags",
        28: "Signer's User ID",
        29: "Reason for Revocation",
        30: "Features",
        31: "Signature Target",
        32: "Embedded Signature",
    }

    @property
    def name(self):
        if self.subtype in (0, 1, 8, 13, 14, 15, 17, 18, 19):
            return "Reserved"
        return self.subpacket_types.get(self.subtype, "Unknown")

    def __repr__(self):
        extra = ""
        if self.hashed:
            extra += "hashed, "
        if self.critical:
            extra += "critical, "
        return "<%s: %s, %slength %d>" % (
                self.__class__.__name__, self.name, extra, self.length)


class SignaturePacket(Packet, AlgoLookup):
    def __init__(self, *args, **kwargs):
        self.sig_version = None
        self.raw_sig_type = None
        self.raw_pub_algorithm = None
        self.raw_hash_algorithm = None
        self.raw_creation_time = None
        self.creation_time = None
        self.raw_expiration_time = None
        self.expiration_time = None
        self.key_id = None
        self.hash2 = None
        self.subpackets = []
        super(SignaturePacket, self).__init__(*args, **kwargs)

    def parse(self):
        self.sig_version = self.data[0]
        offset = 1
        if self.sig_version in (2, 3):
            # 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f
            # |  |  [  ctime  ] [ key_id                 ] |
            # |  |-type                           pub_algo-|
            # |-hash material
            # 10 11 12
            # |  [hash2]
            # |-hash_algo

            # "hash material" byte must be 0x05
            if self.data[offset] != 0x05:
                raise PgpdumpException("Invalid v3 signature packet")
            offset += 1

            self.raw_sig_type = self.data[offset]
            offset += 1

            self.raw_creation_time = get_int4(self.data, offset)
            self.creation_time = datetime.utcfromtimestamp(
                    self.raw_creation_time)
            offset += 4

            self.key_id = get_key_id(self.data, offset)
            offset += 8

            self.raw_pub_algorithm = self.data[offset]
            offset += 1

            self.raw_hash_algorithm = self.data[offset]
            offset += 1

            self.hash2 = self.data[offset:offset + 2]
            offset += 2

        elif self.sig_version == 4:
            # 00 01 02 03 ... <hashedsubpackets..> <subpackets..> [hash2]
            # |  |  |-hash_algo
            # |  |-pub_algo
            # |-type

            self.raw_sig_type = self.data[offset]
            offset += 1

            self.raw_pub_algorithm = self.data[offset]
            offset += 1

            self.raw_hash_algorithm = self.data[offset]
            offset += 1

            # next is hashed subpackets
            length = get_int2(self.data, offset)
            offset += 2
            self.parse_subpackets(offset, length, True)
            offset += length

            # followed by subpackets
            length = get_int2(self.data, offset)
            offset += 2
            self.parse_subpackets(offset, length, False)
            offset += length

            self.hash2 = self.data[offset:offset + 2]
            offset += 2
        else:
            raise PgpdumpException("Unsupported signature packet, version %d" %
                    self.sig_version)

        return offset

    def parse_subpackets(self, outer_offset, outer_length, hashed=False):
        offset = outer_offset
        while offset < outer_offset + outer_length:
            # each subpacket is [variable length] [subtype] [data]
            sub_offset, sub_len, sub_part = new_tag_length(self.data, offset)
            # sub_len includes the subtype single byte, knock that off
            sub_len -= 1
            # initial length bytes
            offset += sub_offset

            subtype = self.data[offset]
            offset += 1

            sub_data = self.data[offset:offset + sub_len]
            if len(sub_data) != sub_len:
                raise PgpdumpException(
                        "Unexpected subpackets length: expected %d, got %d" % (
                            sub_len, len(sub_data)))
            subpacket = SignatureSubpacket(subtype, hashed, sub_data)
            if subpacket.subtype == 2:
                self.raw_creation_time = get_int4(subpacket.data, 0)
                self.creation_time = datetime.utcfromtimestamp(
                        self.raw_creation_time)
            elif subpacket.subtype == 3:
                self.raw_expiration_time = get_int4(subpacket.data, 0)
            elif subpacket.subtype == 16:
                self.key_id = get_key_id(subpacket.data, 0)
            offset += sub_len
            self.subpackets.append(subpacket)

        if self.raw_expiration_time:
            self.expiration_time = self.creation_time + timedelta(
                    seconds=self.raw_expiration_time)

    sig_types = {
        0x00: "Signature of a binary document",
        0x01: "Signature of a canonical text document",
        0x02: "Standalone signature",
        0x10: "Generic certification of a User ID and Public Key packet",
        0x11: "Persona certification of a User ID and Public Key packet",
        0x12: "Casual certification of a User ID and Public Key packet",
        0x13: "Positive certification of a User ID and Public Key packet",
        0x18: "Subkey Binding Signature",
        0x19: "Primary Key Binding Signature",
        0x1f: "Signature directly on a key",
        0x20: "Key revocation signature",
        0x28: "Subkey revocation signature",
        0x30: "Certification revocation signature",
        0x40: "Timestamp signature",
        0x50: "Third-Party Confirmation signature",
    }

    @property
    def sig_type(self):
        return self.sig_types.get(self.raw_sig_type, "Unknown")

    @property
    def pub_algorithm(self):
        return self.lookup_pub_algorithm(self.raw_pub_algorithm)

    @property
    def hash_algorithm(self):
        return self.lookup_hash_algorithm(self.raw_hash_algorithm)

    def __repr__(self):
        return "<%s: %s, %s, length %d>" % (
                self.__class__.__name__, self.pub_algorithm,
                self.hash_algorithm, self.length)


class PublicKeyPacket(Packet, AlgoLookup):
    def __init__(self, *args, **kwargs):
        self.pubkey_version = None
        self.fingerprint = None
        self.key_id = None
        self.raw_creation_time = None
        self.creation_time = None
        self.raw_days_valid = None
        self.expiration_time = None
        self.raw_pub_algorithm = None
        self.pub_algorithm_type = None
        self.modulus = None
        self.modulus_bitlen = None
        self.exponent = None
        self.prime = None
        self.group_order = None
        self.group_gen = None
        self.key_value = None
        super(PublicKeyPacket, self).__init__(*args, **kwargs)

    def parse(self):
        self.pubkey_version = self.data[0]
        offset = 1
        if self.pubkey_version in (2, 3):
            self.raw_creation_time = get_int4(self.data, offset)
            self.creation_time = datetime.utcfromtimestamp(
                    self.raw_creation_time)
            offset += 4

            self.raw_days_valid = get_int2(self.data, offset)
            offset += 2
            if self.raw_days_valid > 0:
                self.expiration_time = self.creation_time + timedelta(
                        days=self.raw_days_valid)

            self.raw_pub_algorithm = self.data[offset]
            offset += 1

            offset = self.parse_key_material(offset)
            md5 = hashlib.md5()
            # Key type must be RSA for v2 and v3 public keys
            if self.pub_algorithm_type == "rsa":
                key_id = ('%X' % self.modulus)[-8:].zfill(8)
                self.key_id = key_id.encode('ascii')
                md5.update(get_int_bytes(self.modulus))
                md5.update(get_int_bytes(self.exponent))
            elif self.pub_algorithm_type == "elg":
                # Of course, there are ELG keys in the wild too. This formula
                # for calculating key_id and fingerprint is derived from an old
                # key and there is a test case based on it.
                key_id = ('%X' % self.prime)[-8:].zfill(8)
                self.key_id = key_id.encode('ascii')
                md5.update(get_int_bytes(self.prime))
                md5.update(get_int_bytes(self.group_gen))
            else:
                raise PgpdumpException("Invalid non-RSA v%d public key" %
                        self.pubkey_version)
            self.fingerprint = md5.hexdigest().upper().encode('ascii')
        elif self.pubkey_version == 4:
            sha1 = hashlib.sha1()
            seed_bytes = (0x99, (self.length >> 8) & 0xff, self.length & 0xff)
            sha1.update(pack_data(bytearray(seed_bytes)))
            sha1.update(pack_data(self.data))
            self.fingerprint = sha1.hexdigest().upper().encode('ascii')
            self.key_id = self.fingerprint[24:]

            self.raw_creation_time = get_int4(self.data, offset)
            self.creation_time = datetime.utcfromtimestamp(
                    self.raw_creation_time)
            offset += 4

            self.raw_pub_algorithm = self.data[offset]
            offset += 1

            offset = self.parse_key_material(offset)
        else:
            raise PgpdumpException("Unsupported public key packet, version %d" %
                    self.pubkey_version)

        return offset

    def parse_key_material(self, offset):
        if self.raw_pub_algorithm in (1, 2, 3):
            self.pub_algorithm_type = "rsa"
            # n, e
            self.modulus, offset = get_mpi(self.data, offset)
            self.exponent, offset = get_mpi(self.data, offset)
            # the length of the modulus in bits
            self.modulus_bitlen = int(ceil(log(self.modulus, 2)))
        elif self.raw_pub_algorithm == 17:
            self.pub_algorithm_type = "dsa"
            # p, q, g, y
            self.prime, offset = get_mpi(self.data, offset)
            self.group_order, offset = get_mpi(self.data, offset)
            self.group_gen, offset = get_mpi(self.data, offset)
            self.key_value, offset = get_mpi(self.data, offset)
        elif self.raw_pub_algorithm in (16, 20):
            self.pub_algorithm_type = "elg"
            # p, g, y
            self.prime, offset = get_mpi(self.data, offset)
            self.group_gen, offset = get_mpi(self.data, offset)
            self.key_value, offset = get_mpi(self.data, offset)
        elif 100 <= self.raw_pub_algorithm <= 110:
            # Private/Experimental algorithms, just move on
            pass
        else:
            raise PgpdumpException("Unsupported public key algorithm %d" %
                    self.raw_pub_algorithm)

        return offset

    @property
    def pub_algorithm(self):
        return self.lookup_pub_algorithm(self.raw_pub_algorithm)

    def __repr__(self):
        return "<%s: 0x%s, %s, length %d>" % (
                self.__class__.__name__, self.key_id.decode('ascii'),
                self.pub_algorithm, self.length)


class PublicSubkeyPacket(PublicKeyPacket):
    '''A Public-Subkey packet (tag 14) has exactly the same format as a
    Public-Key packet, but denotes a subkey.'''
    pass


class SecretKeyPacket(PublicKeyPacket):
    s2k_types = {
        # (Name, Length)
        0: ("Simple S2K", 2),
        1: ("Salted S2K", 10),
        2: ("Reserved value", 0),
        3: ("Iterated and Salted S2K", 11),
        101: ("GnuPG S2K", 6),
    }

    def __init__(self, *args, **kwargs):
        self.s2k_id = None
        self.s2k_type = None
        self.s2k_cipher = None
        self.s2k_hash = None
        self.s2k_iv = None
        self.checksum = None
        self.serial_number = None
        # RSA fields
        self.exponent_d = None
        self.prime_p = None
        self.prime_q = None
        self.multiplicative_inverse = None
        # DSA and Elgamal
        self.exponent_x = None
        super(SecretKeyPacket, self).__init__(*args, **kwargs)

    @classmethod
    def lookup_s2k(cls, s2k_type_id):
        return cls.s2k_types.get(s2k_type_id, ("Unknown", 0))

    def parse(self):
        # parse the public part
        offset = super(SecretKeyPacket, self).parse()

        # parse secret-key packet format from section 5.5.3
        self.s2k_id = self.data[offset]
        offset += 1

        if self.s2k_id == 0:
            # plaintext key data
            offset = self.parse_private_key_material(offset)
            self.checksum = get_int2(self.data, offset)
            offset += 2
        elif self.s2k_id in (254, 255):
            # encrypted key data
            cipher_id = self.data[offset]
            offset += 1
            self.s2k_cipher = self.lookup_sym_algorithm(cipher_id)

            # s2k_length is the len of the entire S2K specifier, as per
            # section 3.7.1 in RFC 4880
            # we parse the info inside the specifier, but verify the # of
            # octects we've parsed matches the expected length of the s2k
            offset_before_s2k = offset

            s2k_type_id = self.data[offset]
            offset += 1
            name, s2k_length = self.lookup_s2k(s2k_type_id)
            self.s2k_type = name

            has_iv = True
            if s2k_type_id == 0:
                # simple string-to-key
                hash_id = self.data[offset]
                offset += 1
                self.s2k_hash = self.lookup_hash_algorithm(hash_id)

            elif s2k_type_id == 1:
                # salted string-to-key
                hash_id = self.data[offset]
                offset += 1
                self.s2k_hash = self.lookup_hash_algorithm(hash_id)
                # ignore 8 bytes
                offset += 8

            elif s2k_type_id == 2:
                # reserved
                pass

            elif s2k_type_id == 3:
                # iterated and salted
                hash_id = self.data[offset]
                offset += 1
                self.s2k_hash = self.lookup_hash_algorithm(hash_id)
                # ignore 8 bytes
                offset += 8
                # ignore count
                offset += 1
                # TODO: parse and store count ?

            elif 100 <= s2k_type_id <= 110:
                # GnuPG string-to-key
                # According to g10/parse-packet.c near line 1832, the 101 packet
                # type is a special GnuPG extension.  This S2K extension is
                # 6 bytes in total:
                #
                #   Octet 0:   101
                #   Octet 1:   hash algorithm
                #   Octet 2-4: "GNU"
                #   Octet 5:   mode integer
                hash_id = self.data[offset]
                offset += 1
                self.s2k_hash = self.lookup_hash_algorithm(hash_id)

                gnu = self.data[offset:offset + 3]
                offset += 3
                if gnu != bytearray(b"GNU"):
                    raise PgpdumpException(
                            "S2K parsing error: expected 'GNU', got %s" % gnu)

                mode = self.data[offset]
                mode += 1000
                offset += 1
                if mode == 1001:
                    has_iv = False
                elif mode == 1002:
                    has_iv = False

                    serial_len = self.data[offset]
                    if serial_len < 0:
                        raise PgpdumpException(
                                "Unexpected serial number length: %d" %
                                serial_len)

                    self.serial_number = get_hex_data(self.data, offset + 1,
                            serial_len)
                else:
                    # TODO implement other modes?
                    raise PgpdumpException(
                            "Unsupported GnuPG S2K extension, encountered mode %d" % mode)
            else:
                raise PgpdumpException(
                        "Unsupported public key algorithm %d" % s2k_type_id)

            if s2k_length != (offset - offset_before_s2k):
                raise PgpdumpException(
                        "Error parsing string-to-key specifier, mismatched length")

            if has_iv:
                s2k_iv_len = self.lookup_sym_algorithm_iv(cipher_id)
                self.s2k_iv = self.data[offset:offset + s2k_iv_len]
                offset += s2k_iv_len

            # TODO decrypt key data
            # TODO parse checksum
            return offset

    def parse_private_key_material(self, offset):
        if self.raw_pub_algorithm in (1, 2, 3):
            self.pub_algorithm_type = "rsa"
            # d, p, q, u
            self.exponent_d, offset = get_mpi(self.data, offset)
            self.prime_p, offset = get_mpi(self.data, offset)
            self.prime_q, offset = get_mpi(self.data, offset)
            self.multiplicative_inverse, offset = get_mpi(self.data, offset)
        elif self.raw_pub_algorithm == 17:
            self.pub_algorithm_type = "dsa"
            # x
            self.exponent_x, offset = get_mpi(self.data, offset)
        elif self.raw_pub_algorithm in (16, 20):
            self.pub_algorithm_type = "elg"
            # x
            self.exponent_x, offset = get_mpi(self.data, offset)
        elif 100 <= self.raw_pub_algorithm <= 110:
            # Private/Experimental algorithms, just move on
            pass
        else:
            raise PgpdumpException("Unsupported public key algorithm %d" %
                    self.raw_pub_algorithm)

        return offset


class SecretSubkeyPacket(SecretKeyPacket):
    '''A Secret-Subkey packet (tag 7) has exactly the same format as a
    Secret-Key packet, but denotes a subkey.'''
    pass


class UserIDPacket(Packet):
    '''A User ID packet consists of UTF-8 text that is intended to represent
    the name and email address of the key holder. By convention, it includes an
    RFC 2822 mail name-addr, but there are no restrictions on its content.'''
    def __init__(self, *args, **kwargs):
        self.user = None
        self.user_name = None
        self.user_email = None
        super(UserIDPacket, self).__init__(*args, **kwargs)

    user_re = re.compile(r'^([^<]+)? ?<([^>]*)>?')

    def parse(self):
        self.user = self.data.decode('utf8', 'replace')
        matches = self.user_re.match(self.user)
        if matches:
            if matches.group(1):
                self.user_name = matches.group(1).strip()
            if matches.group(2):
                self.user_email = matches.group(2).strip()

        return self.length

    def __repr__(self):
        return "<%s: %r (%r), length %d>" % (
                self.__class__.__name__, self.user_name, self.user_email,
                self.length)


class UserAttributePacket(Packet):
    def __init__(self, *args, **kwargs):
        self.raw_image_format = None
        self.image_format = None
        self.image_data = None
        super(UserAttributePacket, self).__init__(*args, **kwargs)

    def parse(self):
        offset = sub_offset = sub_len = 0
        while offset + sub_len < self.length:
            # each subpacket is [variable length] [subtype] [data]
            sub_offset, sub_len, sub_part = new_tag_length(self.data, offset)
            # sub_len includes the subtype single byte, knock that off
            sub_len -= 1
            # initial length bytes
            offset += sub_offset

            sub_type = self.data[offset]
            offset += 1

            # there is only one currently known type- images (1)
            if sub_type == 1:
                # the only little-endian encoded value in OpenPGP
                hdr_size = self.data[offset] + (self.data[offset + 1] << 8)
                hdr_version = self.data[offset + 2]
                self.raw_image_format = self.data[offset + 3]
                offset += hdr_size

                self.image_data = self.data[offset:]
                if self.raw_image_format == 1:
                    self.image_format = "jpeg"
                else:
                    self.image_format = "unknown"

        return self.length


class TrustPacket(Packet):
    def __init__(self, *args, **kwargs):
        self.trust = None
        super(TrustPacket, self).__init__(*args, **kwargs)

    def parse(self):
        '''GnuPG public keyrings use a 2-byte trust value that appears to be
        integer values into some internal enumeration.'''
        if self.length == 2:
            self.trust = get_int2(self.data, 0)
            return 2
        return 0


class PublicKeyEncryptedSessionKeyPacket(Packet, AlgoLookup):
    def __init__(self, *args, **kwargs):
        self.session_key_version = None
        self.key_id = None
        self.raw_pub_algorithm = None
        self.pub_algorithm = None
        super(PublicKeyEncryptedSessionKeyPacket, self).__init__(
                *args, **kwargs)

    def parse(self):
        self.session_key_version = self.data[0]
        if self.session_key_version == 3:
            self.key_id = get_key_id(self.data, 1)
            self.raw_pub_algorithm = self.data[9]
            self.pub_algorithm = self.lookup_pub_algorithm(self.raw_pub_algorithm)
        else:
            raise PgpdumpException(
                    "Unsupported encrypted session key packet, version %d" %
                    self.session_key_version)

        # this is hardcoded to work with the only known session key version
        return 10

    def __repr__(self):
        return "<%s: 0x%s (%s), length %d>" % (
                self.__class__.__name__, self.key_id, self.pub_algorithm,
                self.length)


TAG_TYPES = {
    # (Name, PacketType) tuples
    0:  ("Reserved", None),
    1:  ("Public-Key Encrypted Session Key Packet",
         PublicKeyEncryptedSessionKeyPacket),
    2:  ("Signature Packet", SignaturePacket),
    3:  ("Symmetric-Key Encrypted Session Key Packet", None),
    4:  ("One-Pass Signature Packet", None),
    5:  ("Secret Key Packet", SecretKeyPacket),
    6:  ("Public Key Packet", PublicKeyPacket),
    7:  ("Secret Subkey Packet", SecretSubkeyPacket),
    8:  ("Compressed Data Packet", None),
    9:  ("Symmetrically Encrypted Data Packet", None),
    10: ("Marker Packet", None),
    11: ("Literal Data Packet", None),
    12: ("Trust Packet", TrustPacket),
    13: ("User ID Packet", UserIDPacket),
    14: ("Public Subkey Packet", PublicSubkeyPacket),
    17: ("User Attribute Packet", UserAttributePacket),
    18: ("Symmetrically Encrypted and MDC Packet", None),
    19: ("Modification Detection Code Packet", None),
    60: ("Private", None),
    61: ("Private", None),
    62: ("Private", None),
    63: ("Private", None),
}


def new_tag_length(data, start):
    '''Takes a bytearray of data as input, as well as an offset of where to
    look. Returns a derived (offset, length, partial) tuple.
    Reference: http://tools.ietf.org/html/rfc4880#section-4.2.2
    '''
    first = data[start]
    offset = length = 0
    partial = False

    # one-octet
    if first < 192:
        offset = 1
        length = first

    # two-octet
    elif first < 224:
        offset = 2
        length = ((first - 192) << 8) + data[start + 1] + 192

    # five-octet
    elif first == 255:
        offset = 5
        length = get_int4(data, start + 1)

    # Partial Body Length header, one octet long
    else:
        offset = 1
        # partial length, 224 <= l < 255
        length = 1 << (first & 0x1f)
        partial = True

    return (offset, length, partial)


def old_tag_length(data, start):
    '''Takes a bytearray of data as input, as well as an offset of where to
    look. Returns a derived (offset, length) tuple.'''
    offset = length = 0
    temp_len = data[start] & 0x03

    if temp_len == 0:
        offset = 1
        length = data[start + 1]
    elif temp_len == 1:
        offset = 2
        length = get_int2(data, start + 1)
    elif temp_len == 2:
        offset = 4
        length = get_int4(data, start + 1)
    elif temp_len == 3:
        length = len(data) - start - 1

    return (offset, length)


def construct_packet(data, header_start):
    '''Returns a (length, packet) tuple constructed from 'data' at index
    'header_start'. If there is a next packet, it will be found at
    header_start + length.'''

    # tag encoded in bits 5-0 (new packet format)
    # 0x3f == 111111b
    tag = data[header_start] & 0x3f

    # the header is in new format if bit 7 is set
    # 0x40 == 1000000b
    new = bool(data[header_start] & 0x40)

    if new:
        # length is encoded in the second (and following) octet
        data_offset, data_length, partial = new_tag_length(
                data, header_start + 1)
    else:
        # tag encoded in bits 5-2, discard bits 1-0
        tag >>= 2
        data_offset, data_length = old_tag_length(data, header_start)
        partial = False

    name, PacketType = TAG_TYPES.get(tag, ("Unknown", None))
    # Packet type not yet handled
    if not PacketType:
        PacketType = Packet

    # first octet of the packet header handled
    data_offset += 1

    # data consumed to create new packet, consists of header and data
    consumed = 0
    packet_data = bytearray()
    while (True):
        consumed += data_offset

        data_start = header_start + data_offset
        header_start = data_start + data_length
        packet_data += data[data_start:header_start]
        consumed += data_length

        # The new format might encode data with Partial Body Length headers.
        # Then a packet consists of alternating header and data regions. The
        # last header of a packet is not a Partial Body Length header.
        if partial:
            data_offset, data_length, partial = new_tag_length(
                    data, header_start)
        else:
            break
    packet = PacketType(tag, name, new, packet_data)
    return (consumed, packet)
