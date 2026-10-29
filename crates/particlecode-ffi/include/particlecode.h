/*
 * particlecode.h - C ABI of particlecode-ffi (libparticlecode_ffi.{so,dylib,dll,a}).
 *
 * Written by hand to match crates/particlecode-ffi/src/capi.rs exactly; a Rust test
 * checks that every exported function is declared here.
 *
 * Conventions
 *  - Handles are opaque; create with *_new, release with the matching *_free.
 *    A handle must not be used from two threads at once.
 *  - Pointer-returning functions return NULL on failure. Status-returning functions
 *    return a negative PC_ERR_* value on error.
 *  - Strings returned by the library are NUL-terminated UTF-8; release them with
 *    pc_string_free. Byte buffers are released with pc_bytes_free.
 *  - NULL pointers are always checked and reported. A non-NULL pointer that is invalid,
 *    or a length larger than its buffer, is undefined behaviour.
 *  - No panic crosses this boundary.
 */
#ifndef PARTICLECODE_H
#define PARTICLECODE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Bumped on any incompatible change; compare with pc_abi_version() at load time. */
#define PC_ABI_VERSION 1

/* Errors (negative statuses). */
#define PC_ERR_NULL (-1)    /* a pointer that must not be NULL was NULL */
#define PC_ERR_PANIC (-2)   /* a panic was caught (a bug); discard the handle */
#define PC_ERR_INVALID (-3) /* an argument had an invalid value */

/* pc_decoder_feed_symbols results. */
#define PC_REJECTED 0 /* frame not used; see pc_decoder_reject_reason */
#define PC_ACCEPTED 1 /* frame used; see pc_decoder_progress */
#define PC_DONE 2     /* whole payload arrived; fetch it with pc_decoder_take_ready */

typedef struct PcEncoder PcEncoder;
typedef struct PcDecoder PcDecoder;

/* Transfer progress after the last pc_decoder_feed_symbols call. */
typedef struct PcProgress {
    int32_t status;          /* PC_REJECTED / PC_ACCEPTED / PC_DONE */
    uint32_t have;           /* distinct symbols held, summed over segments */
    uint32_t need;           /* symbols needed if nothing is lost */
    uint32_t segments_done;  /* segments decoded and verified */
    uint32_t segments_total; /* segments in the transfer */
} PcProgress;

/* ---- library ---------------------------------------------------------------------- */

uint32_t pc_abi_version(void);

/* Release a string returned by this library. NULL is a no-op. */
void pc_string_free(char *s);

/* Release a buffer returned by pc_decoder_take_ready. NULL is a no-op. */
void pc_bytes_free(uint8_t *ptr, size_t len);

/*
 * 1 if `rolling` is the liveness value of some second within `skew` seconds of
 * `now_unix` for the 32-byte `key`, 0 if not, PC_ERR_NULL for a NULL key, PC_ERR_INVALID
 * for a key length other than 32.
 */
int32_t pc_liveness_verify(const uint8_t *key, size_t key_len, uint16_t rolling,
                           uint64_t now_unix, uint64_t skew);

/* ---- encoder ---------------------------------------------------------------------- */

/*
 * Create an encoder for `payload_len` bytes at `payload_ptr`.
 *
 * `config_json_ptr`/`config_len` is a JSON EncoderConfig, every field optional ("{}" or
 * NULL with length 0 means all defaults; see the crate README). On failure returns NULL
 * and, if `out_err` is non-NULL, stores a message there (release with pc_string_free);
 * on success `*out_err` is set to NULL.
 */
PcEncoder *pc_encoder_new(const uint8_t *payload_ptr, size_t payload_len,
                          const char *config_json_ptr, size_t config_len, char **out_err);

/* Release an encoder. NULL is a no-op. */
void pc_encoder_free(PcEncoder *enc);

/*
 * Frame `i` as JSON (the serde form of the core FrameSpec), with the liveness clock
 * derived from the index. Release with pc_string_free. NULL on a NULL handle.
 */
char *pc_encoder_frame_json(const PcEncoder *enc, uint32_t i);

/* Frame `i` as JSON, particles showing the liveness value of `unix_seconds`. */
char *pc_encoder_frame_json_at(const PcEncoder *enc, uint32_t i, uint64_t unix_seconds);

/* Session facts (profile, shape, dot count, block size, segments, ...) as JSON. */
char *pc_encoder_info_json(const PcEncoder *enc);

/* Frames in one pass over all segments; 0 on a NULL handle. */
uint32_t pc_encoder_frames_per_pass(const PcEncoder *enc);

/* Copy the 32-byte session key to `out` (32 writable bytes). Returns 0 or PC_ERR_NULL. */
int32_t pc_encoder_key(const PcEncoder *enc, uint8_t *out);

/* ---- decoder ---------------------------------------------------------------------- */

PcDecoder *pc_decoder_new(void);

/* Release a decoder. NULL is a no-op. */
void pc_decoder_free(PcDecoder *dec);

/*
 * Feed one frame: `len` symbols (48 header dots, then the body dots), each a byte holding
 * the 3-bit value (color << 1 | size). `confidences` is NULL ("certain") or `len` bytes of
 * classifier confidence (low values become Reed-Solomon erasures).
 * Returns PC_REJECTED, PC_ACCEPTED, PC_DONE, or a negative error.
 */
int32_t pc_decoder_feed_symbols(PcDecoder *dec, const uint8_t *symbols,
                                const uint8_t *confidences, size_t len);

/* Progress after the last feed. Returns 0 or PC_ERR_NULL. */
int32_t pc_decoder_progress(const PcDecoder *dec, PcProgress *out);

/* Why the last feed returned PC_REJECTED; NULL if it did not. Release with pc_string_free. */
char *pc_decoder_reject_reason(const PcDecoder *dec);

/*
 * Take the payload bytes delivered in order since the last call. Returns 1 and sets
 * *out_ptr / *out_len (release with pc_bytes_free) if there are bytes, 0 and a NULL
 * buffer if there are none, or a negative error. After PC_DONE the concatenation of
 * everything taken equals the payload.
 */
int32_t pc_decoder_take_ready(PcDecoder *dec, uint8_t **out_ptr, size_t *out_len);

/* Session id being received: returns 1 and sets *out once a frame was accepted, else 0. */
int32_t pc_decoder_session_id(const PcDecoder *dec, uint16_t *out);

#ifdef __cplusplus
}
#endif

#endif /* PARTICLECODE_H */
