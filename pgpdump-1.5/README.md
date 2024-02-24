# python-pgpdump: a Python library for parsing PGP packets

This is based on the C version published at:
http://www.mew.org/~kazu/proj/pgpdump/

The intent here is not on completeness, as we don't currently decode every
packet type, but on being able to do what people actually have to 95% of the
time. Currently supported things include:

* Signature packets
* Public key packets
* Secret key packets
* Trust, user ID, and user attribute packets
* ASCII-armor decoding and CRC check

A single codebase with dependencies on only the standard python library is
compatible across Python 2.6, 2.7, and 3.2+, as well as with PyPy 1.8+.
