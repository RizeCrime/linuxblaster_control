# g6ct LADSPA plugin (vendored)

The Creative MatrixEncoder 7.1 → binaural downmixer used by the virtual 7.1
sink. Vendored from [g6-re](https://github.com/HyperRamzey/g6-re) `port/`
so the app can ship it and install it without a source checkout or a network
download.

- `g6ct.c` — the downmixer itself, a 1:1 code port of the G6 downmixer APO
  (`CMatrixEncoderEfxMod` from `KSUSBAPO64.dll`).
- `g6ct_ladspa.c` — LADSPA wrapper, label `g6ct_matrix_encoder`.
- `g6ct.h` — shared types and constants.

`build.rs` compiles the f64 variant (`-DG6CT_DOUBLE`, `-ffp-contract=off`)
into `libg6ct_ladspa_f64.so` and the binary carries it as embedded bytes.

MIT, same as the app. See `LICENSE` in this directory.
