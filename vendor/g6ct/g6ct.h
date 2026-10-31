/*
 * g6ct.h - Sound BlasterX G6 / Creative "MatrixEncoder" downmixer
 *
 * Bit-exact float replica of CMatrixEncoderEfxMod as found in
 * KSUSBAPO64.dll (Creative Sound Blaster USB Audio Drivers, AMD64).
 *
 * Provenance (RVA in KSUSBAPO64.dll, image base 0x180000000):
 *   ctor            sub_1800678F0   (writes the 8 all-pass coefficient blocks)
 *   Process         sub_180067D80   (matrix + all-pass cascade)
 *   biquad          sub_180067D00
 *   coefficients    this+0x0E8 .. this+0x1E4, 8 * 36 bytes
 *   matrix consts   0x1801756D8..0x1801756E8 and 0x180173CD8
 *
 * Node chain that reaches it (device MALCOLM = APOINFO_HW_MALCOLM = 38,
 * internal form factor 2 = headphones, flag 0):
 *   chain 37 = SWAP | CMatrixEncoder(15) | SWAP
 *
 * Wire format on the G6 endpoint: 8ch float 48 kHz mask 0x63F in,
 * 2ch float 48 kHz mask 0x3 out.
 */
#ifndef G6CT_H
#define G6CT_H

#include <stddef.h>

/*
 * Arithmetic precision.
 *
 * The original module runs in IEEE-754 single precision, and the default
 * build reproduces it bit for bit (see ../apo-harness/verification_dll_wine.txt).
 * Defining G6CT_DOUBLE at compile time switches the coefficients, the state
 * and the accumulation to double.  The coefficients are still the exact float
 * values the Creative constructor writes (promoted losslessly), so the
 * transfer function is unchanged - only the arithmetic noise floor moves
 * down.  Measured against an 80-bit reference render, the residual is
 * -104.0 dBFS rms / -93.0 dBFS peak for float32 and -164.0 dBFS rms /
 * -147.7 dBFS peak for float64.  The two builds are separate objects (*.so)
 * because their symbols have the same names but different types.
 */
#ifdef G6CT_DOUBLE
typedef double g6ct_real;
#define G6CT_PRECISION "float64"
#else
typedef float g6ct_real;
#define G6CT_PRECISION "float32"
#endif

/* One 2nd-order all-pass section, direct form I with the module's state. */
typedef struct {
    g6ct_real b0, b1, b2;   /* feed-forward (b2 is always 1.0) */
    g6ct_real a1, a2;       /* feed-back, subtracted           */
    g6ct_real x1, x2;       /* input history                   */
    g6ct_real y1, y2;       /* output history                  */
} g6ct_ap2;

/* Channel mask bits (WAVEFORMATEXTENSIBLE / KSAUDIO_SPEAKER_*) */
#define G6CT_FL   0x0001u
#define G6CT_FR   0x0002u
#define G6CT_FC   0x0004u
#define G6CT_LFE  0x0008u
#define G6CT_BL   0x0010u
#define G6CT_BR   0x0020u
#define G6CT_FLC  0x0040u
#define G6CT_FRC  0x0080u
#define G6CT_BC   0x0100u
#define G6CT_SL   0x0200u
#define G6CT_SR   0x0400u
#define G6CT_TC   0x0800u

#define G6CT_MASK_7_1     0x063Fu   /* FL FR FC LFE BL BR SL SR */
#define G6CT_MASK_8CH     0x00FFu   /* FL FR FC LFE BL BR FLC FRC */

typedef struct {
    g6ct_ap2 front_l[2];   /* +0x0E8 A, +0x10C B  (FL/FC/LFE path, left)  */
    g6ct_ap2 sur_l[2];     /* +0x130 C, +0x154 D  (surround path, left)   */
    g6ct_ap2 front_r[2];   /* +0x178 A, +0x19C B  (right)                 */
    g6ct_ap2 sur_r[2];     /* +0x1C0 C, +0x1E4 D  (right)                 */
    unsigned mask;         /* input channel mask                          */
} g6ct_state;

/* Initialise state and coefficients for an input channel mask. */
void g6ct_init(g6ct_state *s, unsigned channel_mask);

/* Drop filter history (coefficients kept). */
void g6ct_reset(g6ct_state *s);

/*
 * Process n frames.
 *
 * in[0..11] are indexed by WAVEFORMATEXTENSIBLE channel-bit position, exactly
 * like the module's internal channel table:
 *   0 FL, 1 FR, 2 FC, 3 LFE, 4 BL, 5 BR, 6 FLC, 7 FRC, 8 BC, 9 SL, 10 SR, 11 TC
 * A NULL pointer is treated as silence for that channel.
 * outL/outR receive n frames each.
 */
void g6ct_process(g6ct_state *s, const g6ct_real *const in[12],
                  g6ct_real *outL, g6ct_real *outR, size_t n);

/* Arithmetic precision this translation unit was built with. */
const char *g6ct_precision(void);

/* Magnitude response of one all-pass section at angular frequency w. */
double g6ct_ap2_mag(const g6ct_ap2 *s, double w);

#endif /* G6CT_H */
