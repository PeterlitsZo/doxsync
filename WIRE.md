# WIRE

The versions below describe implemented protocol support. Planned features in
`ROADMAP.md` do not change the wire protocol until implemented.

- `1`: Support `v0.1.0-alpha.1`, `v0.1.0-alpha.2`, `v0.1.0-alpha.3`,
  `v0.1.0-alpha.4`, `[UNRELEASED]`.
- `2`: Support `v0.1.0-alpha.3`, `v0.1.0-alpha.4`, `[UNRELEASED]`.
- `3`: Support `v0.1.0-alpha.4`, `[UNRELEASED]`.
- `4`: Support `[UNRELEASED]`.

Protocol 4 adds AOO column modes `4` (`DENSE_BOOL`, packed Boolean bits),
`5` (`DENSE_INT`, original ZigZag integers), `6` (`DENSE_DECIMAL`, a shared
scale and original ZigZag coefficients), and `7` (`DENSE_STRING`, UTF-8
literals or string pool references). Existing modes `0`–`3` remain supported.
The encoder selects the shortest eligible column encoding, retaining ordinary
values on ties and comparing original numeric values against deltas.
