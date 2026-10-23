/*
 * g6ct.c - Creative "MatrixEncoder" 7.1 -> binaural stereo downmixer.
 *
 * Structure recovered from sub_180067D80 (KSUSBAPO64.dll):
 *
 *   frontL = FL + 0.707*FC + 0.707*LFE
 *   frontR = FR + 0.707*FC + 0.707*LFE
 *   surL   = 0.786*BL + 0.619*BR + 0.875*SL + 0.485*SR   (+0.707*BC)
 *   surR   = 0.619*BL + 0.786*BR + 0.485*SL + 0.875*SR   (+0.707*BC)
 *   outL   = B(A(frontL)) + D(C(surL))
 *   outR   = B(A(frontR)) - D(C(surR))
 *
 * A..D are 2nd-order all-passes (b0 == a2, b1 == a1, b2 == 1), so each
 * branch is phase-only.  The minus sign on the right surround branch is
 * the out-of-phase cross-feed that puts the surround image behind the
 * listener.  Every block adds 1e-10 to its input (denormal guard in the
 * original).
 */
#include "g6ct.h"

#include <math.h>
#include <string.h>

#if defined(__FAST_MATH__)
#error "g6ct.c must not be built with -ffast-math (the float32 build is the bit-exact DLL replica)"
#endif

/* Bit pattern -> float -> g6ct_real.  float -> double is exact, so the
 * double build uses precisely the constant the DLL stores. */
static g6ct_real u2r(unsigned u)
{
    float f;
    memcpy(&f, &u, sizeof f);
    return (g6ct_real)f;
}

/* Exact float bit patterns from the module's constructor. */
#define AP_A_B0 0x3F725269u
#define AP_A_B1 0xBFF92104u
#define AP_B_B0 0x3D82D384u
#define AP_B_B1 0xBF567621u
#define AP_C_B0 0xBE856085u
#define AP_C_B1 0x3CD273D6u
#define AP_D_B0 0x3F5EE547u
#define AP_D_B1 0xBFEF2B02u
#define AP_B2   0x3F800000u
#define EPSILON 0x2EDBE6FFu   /* 1.0000000134e-10 */

#define F_707 0x3F34FDF4u    /* 0.7070000171661377 */
#define F_485 0x3EF851ECu    /* 0.48500001430511475 */
#define F_619 0x3F1E76C9u    /* 0.6190000176429749  */
#define F_786 0x3F49374Cu    /* 0.7860000133514404  */
#define F_875 0x3F600000u    /* 0.875               */

static void ap2_set(g6ct_ap2 *s, unsigned b0, unsigned b1)
{
    memset(s, 0, sizeof *s);
    s->b0 = u2r(b0);
    s->b1 = u2r(b1);
    s->b2 = u2r(AP_B2);
    s->a1 = u2r(b1);
    s->a2 = u2r(b0);
}

void g6ct_init(g6ct_state *s, unsigned channel_mask)
{
    memset(s, 0, sizeof *s);
    s->mask = channel_mask;

    ap2_set(&s->front_l[0], AP_A_B0, AP_A_B1);
    ap2_set(&s->front_l[1], AP_B_B0, AP_B_B1);
    ap2_set(&s->sur_l[0],   AP_C_B0, AP_C_B1);
    ap2_set(&s->sur_l[1],   AP_D_B0, AP_D_B1);
    ap2_set(&s->front_r[0], AP_A_B0, AP_A_B1);
    ap2_set(&s->front_r[1], AP_B_B0, AP_B_B1);
    ap2_set(&s->sur_r[0],   AP_C_B0, AP_C_B1);
    ap2_set(&s->sur_r[1],   AP_D_B0, AP_D_B1);
}

void g6ct_reset(g6ct_state *s)
{
    unsigned m = s->mask;
    g6ct_init(s, m);
}

/* y = b0*x + b1*x1 + b2*x2 - a1*y1 - a2*y2, input guarded by +1e-10. */
static g6ct_real ap2_run(g6ct_ap2 *s, g6ct_real x)
{
    /* The guard is the same value the DLL adds (a float constant, promoted
     * exactly in the double build). */
    const g6ct_real in = x + u2r(EPSILON);
    g6ct_real y = s->b0 * in + s->b1 * s->x1 + s->b2 * s->x2
                - s->a1 * s->y1 - s->a2 * s->y2;

    s->x2 = s->x1;
    s->x1 = in;
    s->y2 = s->y1;
    s->y1 = y;
    return y;
}

void g6ct_process(g6ct_state *s, const g6ct_real *const in[12],
                  g6ct_real *outL, g6ct_real *outR, size_t n)
{
    const unsigned m = s->mask;
    const g6ct_real k707 = u2r(F_707), k485 = u2r(F_485), k619 = u2r(F_619);
    const g6ct_real k786 = u2r(F_786), k875 = u2r(F_875);
    const int has_fc  = (m & G6CT_FC)  != 0;
    const int has_lfe = (m & G6CT_LFE) != 0;
    const int has_bc  = (m & G6CT_BC)  != 0;
    const int has_ss  = (m & G6CT_SL) && (m & G6CT_SR);
    const int has_bs  = (m & G6CT_BL) && (m & G6CT_BR);
    size_t i;

    /* The DLL has a stereo-bypass branch: an input mask of exactly FL|FR
     * returns the front pair untouched (verified bit-for-bit under Wine).
     * Mask 0, and masks with any other channel bit, run the downmixer. */
    if (m == (G6CT_FL | G6CT_FR)) {
        for (i = 0; i < n; i++) {
            outL[i] = in[0] ? in[0][i] : 0;
            outR[i] = in[1] ? in[1][i] : 0;
        }
        return;
    }

    for (i = 0; i < n; i++) {
        g6ct_real fl = in[0] ? in[0][i] : 0;
        g6ct_real fr = in[1] ? in[1][i] : 0;
        g6ct_real sl = 0, sr = 0, t, l, r;

        if (has_fc) {
            t = (in[2] ? in[2][i] : 0) * k707;
            fl += t;
            fr += t;
        }
        if (has_lfe) {
            t = (in[3] ? in[3][i] : 0) * k707;
            fl += t;
            fr += t;
        }
        if (has_bc) {
            sl = (in[8] ? in[8][i] : 0) * k707;
            sr = sl;
        }
        if (has_ss) {
            g6ct_real a = in[9]  ? in[9][i]  : 0;     /* SL */
            g6ct_real b = in[10] ? in[10][i] : 0;     /* SR */
            sl += k875 * a + k485 * b;
            sr += k485 * a + k875 * b;
        }
        if (has_bs) {
            g6ct_real a = in[4] ? in[4][i] : 0;       /* BL */
            g6ct_real b = in[5] ? in[5][i] : 0;       /* BR */
            sl += k619 * b + k786 * a;
            sr += k619 * a + k786 * b;
        }

        l = ap2_run(&s->front_l[0], fl);
        l = ap2_run(&s->front_l[1], l);
        t = ap2_run(&s->sur_l[0], sl);
        t = ap2_run(&s->sur_l[1], t);
        outL[i] = l + t;

        r = ap2_run(&s->front_r[0], fr);
        r = ap2_run(&s->front_r[1], r);
        t = ap2_run(&s->sur_r[0], sr);
        t = ap2_run(&s->sur_r[1], t);
        outR[i] = r - t;
    }
}

const char *g6ct_precision(void)
{
    return G6CT_PRECISION;
}

double g6ct_ap2_mag(const g6ct_ap2 *s, double w)
{
    /* H(z) = (b0 + b1 z^-1 + b2 z^-2) / (1 + a1 z^-1 + a2 z^-2) */
    double c1 = cos(w), s1 = sin(w);
    double c2 = cos(2 * w), s2 = sin(2 * w);
    double nr = s->b0 + s->b1 * c1 + s->b2 * c2;
    double ni = -(s->b1 * s1 + s->b2 * s2);
    double dr = 1.0 + s->a1 * c1 + s->a2 * c2;
    double di = -(s->a1 * s1 + s->a2 * s2);
    double num = nr * nr + ni * ni;
    double den = dr * dr + di * di;

    return den > 0.0 ? sqrt(num / den) : 0.0;
}
