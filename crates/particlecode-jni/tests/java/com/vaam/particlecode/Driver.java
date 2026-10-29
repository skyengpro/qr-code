package com.vaam.particlecode;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Properties;
import java.util.Random;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static com.vaam.particlecode.Layout.*;

/**
 * Drives {@link NativeBridge} the way the Android analyzer does: direct buffers, U and V as
 * overlapping views of one interleaved VU buffer, preallocated result arrays. Also pokes the
 * error paths. Exits non-zero (with a stack trace) on the first failed check.
 */
public final class Driver {
    private static final AtomicInteger CHECKS = new AtomicInteger();

    private static void check(boolean ok, String msg) {
        CHECKS.incrementAndGet();
        if (!ok) {
            throw new AssertionError(msg);
        }
    }

    private static void eq(long want, long got, String msg) {
        check(want == got, msg + ": want " + want + " got " + got);
    }

    /** One camera frame as Android delivers NV21: padded luma and an interleaved VU buffer. */
    private static final class Frame {
        final int index;
        final byte[] y;
        final byte[] vu;

        Frame(int index, byte[] y, byte[] vu) {
            this.index = index;
            this.y = y;
            this.vu = vu;
        }
    }

    private static final class Meta {
        int width;
        int height;
        int yStride;
        int uvStride;
        int uvPixelStride;
        long origin;
        int sessionId;
        int fps;
        byte[] key;
        byte[] payload;
        List<Frame> frames = new ArrayList<>();
    }

    private static Meta load(Path dir) throws IOException {
        Properties p = new Properties();
        try (InputStream in = Files.newInputStream(dir.resolve("meta.properties"))) {
            p.load(in);
        }
        Meta m = new Meta();
        m.width = Integer.parseInt(p.getProperty("width"));
        m.height = Integer.parseInt(p.getProperty("height"));
        m.yStride = Integer.parseInt(p.getProperty("yStride"));
        m.uvStride = Integer.parseInt(p.getProperty("uvStride"));
        m.uvPixelStride = Integer.parseInt(p.getProperty("uvPixelStride"));
        m.origin = Long.parseLong(p.getProperty("origin"));
        m.sessionId = Integer.parseInt(p.getProperty("sessionId"));
        m.fps = Integer.parseInt(p.getProperty("fps"));
        m.key = Files.readAllBytes(dir.resolve("key.bin"));
        m.payload = Files.readAllBytes(dir.resolve("payload.bin"));
        for (String s : p.getProperty("indices").split(",")) {
            int i = Integer.parseInt(s.trim());
            m.frames.add(new Frame(i,
                    Files.readAllBytes(dir.resolve("frame_" + i + ".y")),
                    Files.readAllBytes(dir.resolve("frame_" + i + ".vu"))));
        }
        return m;
    }

    private static ByteBuffer direct(byte[] b) {
        ByteBuffer d = ByteBuffer.allocateDirect(b.length);
        d.put(b);
        d.clear();
        return d;
    }

    /** The U plane of NV21: the VU buffer seen from byte 1 (same memory, no copy). */
    private static ByteBuffer shifted(ByteBuffer vu) {
        ByteBuffer d = vu.duplicate();
        d.position(1);
        return d.slice();
    }

    private static int feed(long h, Meta m, ByteBuffer y, ByteBuffer u, ByteBuffer v, int[] out, float[] geom) {
        return NativeBridge.nativeFeedYuv(h, m.width, m.height, y, u, v,
                m.yStride, m.uvStride, m.uvPixelStride, out, geom);
    }

    private static byte[] filled(int n, byte v) {
        byte[] b = new byte[n];
        Arrays.fill(b, v);
        return b;
    }

    /** Feeds frames until DONE; returns the per-frame out arrays. */
    private static List<int[]> runTransfer(Meta m, boolean viaBytes) {
        long h = NativeBridge.nativeNew();
        check(h > 0, "handle is positive");
        int[] out = new int[OUT_LEN];
        float[] geom = new float[GEOM_LEN];
        ByteArrayOutputStream streamed = new ByteArrayOutputStream();
        List<int[]> outs = new ArrayList<>();
        int lastHave = 0;
        boolean done = false;
        for (Frame f : m.frames) {
            int status;
            if (viaBytes) {
                byte[] u = Arrays.copyOfRange(f.vu, 1, f.vu.length);
                status = NativeBridge.nativeFeedYuvBytes(h, m.width, m.height, f.y, u, f.vu,
                        m.yStride, m.uvStride, m.uvPixelStride, out, geom);
            } else {
                ByteBuffer vu = direct(f.vu);
                status = feed(h, m, direct(f.y), shifted(vu), vu, out, geom);
            }
            check(status >= 0, "frame " + f.index + " status " + status);
            outs.add(out.clone());
            eq(1, out[OUT_LOCKED], "frame " + f.index + " locked");
            eq(1, out[OUT_PROFILE], "profile M");
            check(out[OUT_ANCHOR_COUNT] == 3 || out[OUT_ANCHOR_COUNT] == 4, "anchors found");
            for (int a = 0; a < 2 * out[OUT_ANCHOR_COUNT]; a += 2) {
                float x = geom[GEOM_ANCHORS + a];
                float yy = geom[GEOM_ANCHORS + a + 1];
                check(x > 0 && x < m.width && yy > 0 && yy < m.height,
                        "anchor inside the image: " + x + "," + yy);
            }
            check(geom[GEOM_SHARPNESS] > 0f, "sharpness");
            eq(1, out[OUT_LIVENESS_READ], "liveness read");
            eq(1, out[OUT_LIVENESS_CONSISTENT], "liveness consistent");
            long second = m.origin + f.index / m.fps;
            check(NativeBridge.nativeVerifyLiveness(m.key, out[OUT_ROLLING], second, 0),
                    "particles show the liveness of second " + second);
            check(out[OUT_HAVE] >= lastHave, "progress never goes backwards");
            lastHave = out[OUT_HAVE];
            check(out[OUT_SEGMENTS_TOTAL] >= 1, "segments total");
            eq(HINT_NONE, out[OUT_HINT], "no hint on a good frame");
            check(NativeBridge.nativeRejectReason(h) == null, "no reject reason on a good frame");

            byte[] ready = NativeBridge.nativeTakeReady(h);
            if (ready != null) {
                streamed.write(ready, 0, ready.length);
            }
            if (status == STATUS_DONE) {
                eq(out[OUT_NEED], out[OUT_HAVE], "have == need when done");
                eq(out[OUT_SEGMENTS_TOTAL], out[OUT_SEGMENTS_DONE], "all segments done");
                byte[] payload = NativeBridge.nativePayload(h);
                check(payload != null, "payload available");
                check(Arrays.equals(m.payload, payload), "payload bytes match");
                check(Arrays.equals(m.payload, streamed.toByteArray()), "streamed bytes match");
                eq(m.sessionId, NativeBridge.nativeSessionId(h), "session id");
                done = true;
                break;
            }
            check(NativeBridge.nativePayload(h) == null, "no payload before DONE");
        }
        check(done, "transfer completed (viaBytes=" + viaBytes + ")");

        // Reset forgets everything and the handle is reusable.
        eq(0, NativeBridge.nativeReset(h), "reset");
        check(NativeBridge.nativePayload(h) == null, "payload cleared by reset");
        eq(-1, NativeBridge.nativeSessionId(h), "session cleared by reset");
        Frame first = m.frames.get(0);
        ByteBuffer vu = direct(first.vu);
        check(feed(h, m, direct(first.y), shifted(vu), vu, out, geom) >= 0, "feed after reset");
        eq(m.sessionId, NativeBridge.nativeSessionId(h), "session id after reset + feed");

        NativeBridge.nativeFree(h);
        NativeBridge.nativeFree(h); // double free is a no-op
        eq(ERR_NULL, NativeBridge.nativeFeedYuvBytes(h, m.width, m.height, first.y, first.vu, first.vu,
                m.yStride, m.uvStride, m.uvPixelStride, out, geom), "freed handle");
        return outs;
    }

    private static void errorPaths(Meta m) {
        Frame f = m.frames.get(0);
        long h = NativeBridge.nativeNew();
        int[] out = new int[OUT_LEN];
        float[] geom = new float[GEOM_LEN];
        ByteBuffer vu = direct(f.vu);
        ByteBuffer y = direct(f.y);
        ByteBuffer u = shifted(vu);

        // Sanity: the arguments are good.
        check(feed(h, m, y, u, vu, out, geom) >= 0, "baseline feed");
        Arrays.fill(out, 77);
        Arrays.fill(geom, 77f);

        // Handles.
        for (long bad : new long[] {0L, -1L, Long.MIN_VALUE, Long.MAX_VALUE, h + 1000}) {
            eq(ERR_NULL, feed(bad, m, y, u, vu, out, geom), "bad handle " + bad);
            eq(ERR_NULL, NativeBridge.nativeReset(bad), "reset bad handle " + bad);
            check(NativeBridge.nativeTakeReady(bad) == null, "takeReady bad handle");
            check(NativeBridge.nativePayload(bad) == null, "payload bad handle");
            check(NativeBridge.nativeRejectReason(bad) == null, "rejectReason bad handle");
            check(NativeBridge.nativeHint(bad) == null, "hint bad handle");
            eq(-1, NativeBridge.nativeSessionId(bad), "sessionId bad handle");
            NativeBridge.nativeFree(bad);
        }
        eq(ERR_NULL, NativeBridge.nativeFeedYuvBytes(0, m.width, m.height, f.y, f.vu, f.vu,
                m.yStride, m.uvStride, m.uvPixelStride, out, geom), "bytes variant, handle 0");

        // Null buffers and arrays.
        eq(ERR_NULL, feed(h, m, null, u, vu, out, geom), "null y");
        eq(ERR_NULL, feed(h, m, y, null, vu, out, geom), "null u");
        eq(ERR_NULL, feed(h, m, y, u, null, out, geom), "null v");
        eq(ERR_NULL, feed(h, m, y, u, vu, null, geom), "null out");
        eq(ERR_NULL, feed(h, m, y, u, vu, out, null), "null outGeom");
        eq(ERR_NULL, NativeBridge.nativeFeedYuvBytes(h, m.width, m.height, null, f.vu, f.vu,
                m.yStride, m.uvStride, m.uvPixelStride, out, geom), "null y bytes");

        // Non-direct buffers.
        eq(ERR_NOT_DIRECT, feed(h, m, ByteBuffer.wrap(f.y), u, vu, out, geom), "heap y");
        eq(ERR_NOT_DIRECT, feed(h, m, y, ByteBuffer.allocate(f.vu.length), vu, out, geom), "heap u");
        eq(ERR_NOT_DIRECT, feed(h, m, y, u, ByteBuffer.wrap(f.vu), out, geom), "heap v");

        // Short buffers (stated dimensions do not fit).
        eq(ERR_INVALID, feed(h, m, direct(Arrays.copyOf(f.y, f.y.length / 2)), u, vu, out, geom), "short y");
        eq(ERR_INVALID, feed(h, m, y, direct(new byte[10]), vu, out, geom), "short u");
        eq(ERR_INVALID, feed(h, m, y, u, direct(new byte[10]), out, geom), "short v");
        eq(ERR_INVALID, feed(h, m, direct(new byte[0]), u, vu, out, geom), "empty y");
        eq(ERR_INVALID, NativeBridge.nativeFeedYuvBytes(h, m.width, m.height, new byte[5], f.vu, f.vu,
                m.yStride, m.uvStride, m.uvPixelStride, out, geom), "short y bytes");

        // Bad numbers.
        int[][] dims = {{0, m.height}, {m.width, 0}, {-1, m.height}, {m.width, -5},
            {Integer.MAX_VALUE, m.height}, {m.width, Integer.MAX_VALUE}};
        for (int[] d : dims) {
            eq(ERR_INVALID, NativeBridge.nativeFeedYuv(h, d[0], d[1], y, u, vu,
                    m.yStride, m.uvStride, m.uvPixelStride, out, geom), "dims " + d[0] + "x" + d[1]);
        }
        eq(ERR_INVALID, NativeBridge.nativeFeedYuv(h, m.width, m.height, y, u, vu,
                -1, m.uvStride, m.uvPixelStride, out, geom), "negative y stride");
        eq(ERR_INVALID, NativeBridge.nativeFeedYuv(h, m.width, m.height, y, u, vu,
                Integer.MAX_VALUE, m.uvStride, m.uvPixelStride, out, geom), "huge y stride");
        eq(ERR_INVALID, NativeBridge.nativeFeedYuv(h, m.width, m.height, y, u, vu,
                m.yStride, Integer.MAX_VALUE, m.uvPixelStride, out, geom), "huge uv stride");
        eq(ERR_INVALID, NativeBridge.nativeFeedYuv(h, m.width, m.height, y, u, vu,
                m.yStride, m.uvStride, 0, out, geom), "zero uv pixel stride");
        eq(ERR_INVALID, NativeBridge.nativeFeedYuv(h, m.width, m.height, y, u, vu,
                m.yStride, m.uvStride, -2, out, geom), "negative uv pixel stride");
        eq(ERR_INVALID, NativeBridge.nativeFeedYuv(h, m.width, m.height, y, u, vu,
                m.width - 1, m.uvStride, m.uvPixelStride, out, geom), "y stride below width");

        // Result arrays.
        eq(ERR_INVALID, feed(h, m, y, u, vu, new int[OUT_LEN - 1], geom), "short out");
        eq(ERR_INVALID, feed(h, m, y, u, vu, out, new float[GEOM_LEN - 1]), "short outGeom");
        eq(ERR_INVALID, feed(h, m, y, u, vu, new int[0], geom), "empty out");

        // None of the failures touched the arrays.
        for (int v : out) {
            eq(77, v, "out untouched by errors");
        }
        for (float g : geom) {
            check(g == 77f, "outGeom untouched by errors");
        }

        // Longer arrays are fine; only the head is written.
        int[] big = new int[OUT_LEN + 5];
        Arrays.fill(big, 77);
        check(feed(h, m, y, u, vu, big, new float[GEOM_LEN + 3]) >= 0, "longer arrays");
        eq(77, big[OUT_LEN], "tail of a longer out array is untouched");

        // A blank frame and a noise frame are rejections with advice, not errors.
        int w = 320;
        int hh = 240;
        ByteBuffer gray = direct(filled(w * hh, (byte) 128));
        ByteBuffer chroma = direct(filled(w * hh / 4, (byte) 128));
        eq(STATUS_REJECTED, NativeBridge.nativeFeedYuv(h, w, hh, gray, chroma, chroma, w, w / 2, 1, out, geom),
                "blank frame");
        eq(0, out[OUT_LOCKED], "blank: not locked");
        eq(-1, out[OUT_PROFILE], "blank: no profile");
        check(out[OUT_HINT] != HINT_NONE, "blank: hint code");
        check(NativeBridge.nativeHint(h) != null, "blank: hint text");
        check(NativeBridge.nativeRejectReason(h) != null, "blank: reject reason");
        byte[] noise = new byte[w * hh];
        new Random(1).nextBytes(noise);
        eq(STATUS_REJECTED,
                NativeBridge.nativeFeedYuv(h, w, hh, direct(noise), chroma, chroma, w, w / 2, 1, out, geom),
                "noise frame");

        // Liveness verification.
        long second = m.origin;
        int rolling = -1;
        for (int r = 0; r < 65536 && rolling < 0; r++) {
            if (NativeBridge.nativeVerifyLiveness(m.key, r, second, 0)) {
                rolling = r;
            }
        }
        check(rolling >= 0, "some rolling value verifies for the origin second");
        check(NativeBridge.nativeVerifyLiveness(m.key, rolling, second + 2, 2), "skew window");
        check(!NativeBridge.nativeVerifyLiveness(m.key, rolling, second + 1000, 0), "outside the window");
        check(!NativeBridge.nativeVerifyLiveness(new byte[32], rolling, second, 0), "wrong key");
        check(!NativeBridge.nativeVerifyLiveness(null, rolling, second, 0), "null key");
        check(!NativeBridge.nativeVerifyLiveness(new byte[31], rolling, second, 0), "short key");
        check(!NativeBridge.nativeVerifyLiveness(new byte[33], rolling, second, 0), "long key");
        check(!NativeBridge.nativeVerifyLiveness(m.key, -1, second, 0), "negative rolling");
        check(!NativeBridge.nativeVerifyLiveness(m.key, 65536, second, 0), "rolling above 16 bits");
        check(!NativeBridge.nativeVerifyLiveness(m.key, rolling, -1, 0), "negative time");
        check(!NativeBridge.nativeVerifyLiveness(m.key, rolling, second, -1), "negative skew");

        NativeBridge.nativeFree(h);
    }

    /** Independent handles on independent threads, and a free racing a feed. */
    private static void threads(Meta m) throws InterruptedException {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        Thread[] ts = new Thread[3];
        for (int i = 0; i < ts.length; i++) {
            ts[i] = new Thread(() -> {
                try {
                    runTransfer(m, false);
                } catch (Throwable t) {
                    failure.compareAndSet(null, t);
                }
            });
            ts[i].start();
        }

        // A free racing feeds on the same handle: statuses are valid ones or ERR_NULL.
        long h = NativeBridge.nativeNew();
        ByteBuffer gray = direct(filled(320 * 240, (byte) 128));
        ByteBuffer chroma = direct(filled(320 * 240 / 4, (byte) 128));
        Thread feeder = new Thread(() -> {
            int[] out = new int[OUT_LEN];
            float[] geom = new float[GEOM_LEN];
            try {
                for (int i = 0; i < 300; i++) {
                    int s = NativeBridge.nativeFeedYuv(h, 320, 240, gray, chroma, chroma, 320, 160, 1, out, geom);
                    check(s == STATUS_REJECTED || s == ERR_NULL, "racing feed status " + s);
                }
            } catch (Throwable t) {
                failure.compareAndSet(null, t);
            }
        });
        feeder.start();
        Thread.sleep(20);
        NativeBridge.nativeFree(h);
        feeder.join();
        for (Thread t : ts) {
            t.join();
        }
        if (failure.get() != null) {
            throw new AssertionError("thread failed", failure.get());
        }
    }

    public static void main(String[] args) throws Exception {
        Meta m = load(Paths.get(args[0]));
        errorPaths(m);
        System.out.println("error paths ok");

        List<int[]> direct = runTransfer(m, false);
        System.out.println("direct buffers: done after " + direct.size() + " frames");
        List<int[]> bytes = runTransfer(m, true);
        check(direct.size() == bytes.size(), "byte[] variant needs the same number of frames");
        for (int i = 0; i < direct.size(); i++) {
            check(Arrays.equals(direct.get(i), bytes.get(i)), "byte[] and direct results agree at frame " + i);
        }
        System.out.println("byte[] variant agrees");

        threads(m);
        System.out.println("threads ok");
        System.out.println("OK " + CHECKS.get() + " checks");
    }
}
