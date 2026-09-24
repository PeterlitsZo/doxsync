# WIRE

- `1`: Support `v0.1.0-alpha.1`, `v0.1.0-alpha.2`, `[UNRELEASED]`.
- `2`: Support `[UNRELEASED]`.

## Floating-point values

Protocol 1 encodes every float as `0x7b` followed by eight little-endian
IEEE 754 binary64 bytes. Protocol 2 additionally accepts `0x79` with two
binary16 bytes and `0x7a` with four binary32 bytes, also little-endian.

Protocol 2 encoders select the shortest representation whose conversion back to
binary64 has identical bits. Signed zero and infinities can use binary16; NaNs
always retain their original binary64 bits, including sign and payload.
Decoders restore all widths to binary64 and also accept wider representations.
Short float tags are rejected under protocol 1. Protocol negotiation
selects the highest shared version, preserving binary64 encoding for older peers.
