package com.example.pinpointer.util

import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.sin

/**
 * Collects (azimuth, elevation, RSSI) samples as the user pans the yagi antenna
 * and estimates the azimuth/elevation that maximises signal strength.
 *
 * ### Algorithm
 * Uses a **cubic-weighted circular mean** for azimuth and a weighted arithmetic
 * mean for elevation.  Each sample is weighted by `(rssi − minRssi + 1)³`, so
 * the peak dominates the estimate while weak readings barely contribute.
 *
 * Azimuth wrap-around is handled correctly via `sin`/`cos` decomposition.
 *
 * ### When is the estimate valid?
 * At least [MIN_SAMPLES] samples have been collected AND the peak-to-floor
 * spread in RSSI exceeds [MIN_RSSI_SPREAD_DB].  This guards against situations
 * where the antenna pattern is too flat to give a reliable bearing.
 */
class RssiDirectionFinder {

    companion object {
        /** Minimum number of samples before an estimate is attempted. */
        const val MIN_SAMPLES = 8

        /** Minimum peak-to-floor RSSI variation (dB) to treat as directional. */
        const val MIN_RSSI_SPREAD_DB = 5

        /** Samples older than this are evicted. */
        private const val MAX_AGE_MS = 90_000L

        /** Don't add a new sample unless azimuth moved at least this many degrees. */
        private const val MIN_AZ_DELTA_DEG = 4f

        /** Number of azimuth bins for coverage tracking (10° each → 36 bins). */
        const val NUM_BINS = 36
    }

    data class Sample(
        val azimuth: Float,   // degrees [0, 360)
        val elevation: Float, // degrees [-90, 90]
        val rssi: Int,
        val timestampMs: Long
    )

    private val samples = ArrayDeque<Sample>()

    // ── Public accessors ──────────────────────────────────────────────────────

    val sampleCount: Int get() = samples.size
    val maxRssi: Int get() = samples.maxOfOrNull { it.rssi } ?: -120

    /**
     * Boolean array of length [NUM_BINS].  Element `i` is `true` if at least
     * one sample landed in the azimuth sector [i*10°, (i+1)*10°).
     */
    val azimuthBins: List<Boolean>
        get() {
            val bins = MutableList(NUM_BINS) { false }
            samples.forEach { s ->
                val bin = ((s.azimuth / 10f).toInt()).coerceIn(0, NUM_BINS - 1)
                bins[bin] = true
            }
            return bins
        }

    /** Fraction of the full 360° that has been scanned, in [0, 1]. */
    val coverage: Float
        get() = azimuthBins.count { it } / NUM_BINS.toFloat()

    /**
     * Best estimated (azimuth, elevation) in degrees, or `null` if there is not
     * yet enough data / variation to trust the estimate.
     */
    val estimate: Pair<Float, Float>?
        get() {
            if (samples.size < MIN_SAMPLES) return null
            val minRssi = samples.minOf { it.rssi }
            val maxRssi = samples.maxOf { it.rssi }
            if (maxRssi - minRssi < MIN_RSSI_SPREAD_DB) return null

            var sinSum = 0.0
            var cosSum = 0.0
            var elSum = 0.0
            var wSum = 0.0

            samples.forEach { s ->
                val w = (s.rssi - minRssi + 1.0).pow(3.0) // cubic: strongly emphasises peak
                sinSum += w * sin(Math.toRadians(s.azimuth.toDouble()))
                cosSum += w * cos(Math.toRadians(s.azimuth.toDouble()))
                elSum += w * s.elevation
                wSum += w
            }

            val az = ((Math.toDegrees(atan2(sinSum, cosSum)) + 360.0) % 360.0).toFloat()
            val el = (elSum / wSum).toFloat()
            return Pair(az, el)
        }

    // ── Mutation ──────────────────────────────────────────────────────────────

    /**
     * Add a new observation.  Stale samples are evicted first.
     * Returns `true` if the sample was actually added (i.e. the azimuth moved
     * enough since the last sample).
     */
    fun addSample(azimuth: Float, elevation: Float, rssi: Int): Boolean {
        val now = System.currentTimeMillis()

        // Evict stale samples
        while (samples.isNotEmpty() && now - samples.first().timestampMs > MAX_AGE_MS) {
            samples.removeFirst()
        }

        // Skip if azimuth hasn't changed meaningfully
        val lastAz = samples.lastOrNull()?.azimuth
        if (lastAz != null && abs(angleDiff(azimuth, lastAz)) < MIN_AZ_DELTA_DEG) return false

        samples.addLast(Sample(azimuth, elevation, rssi, now))
        return true
    }

    /** Clear all accumulated samples (e.g. when GPS becomes available again). */
    fun clear() = samples.clear()

    /** Whether the scan has covered enough of the sky to return a reliable estimate. */
    val isConfident: Boolean
        get() = estimate != null && coverage >= 0.25f

    // ── Private helpers ───────────────────────────────────────────────────────

    /** Shortest signed difference between two compass bearings, in (-180, 180]. */
    private fun angleDiff(a: Float, b: Float): Float =
        (((a - b + 180f + 360f) % 360f) - 180f)
}
