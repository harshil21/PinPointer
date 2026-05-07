package com.example.pinpointer.util

import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.pow
import kotlin.math.sin

/**
 * Collects (azimuth, elevation, RSSI) samples as the user pans the yagi antenna
 * and estimates the azimuth/elevation that maximises signal strength.
 *
 * ### Algorithm
 * Uses a **cubic-weighted circular mean** with **exponential time decay**.
 * Weight = (rssi − minRssi + 1)³ × exp(−age / halfLifeMs).
 * This means older samples fade out quickly, so the estimate updates fast once
 * a high-RSSI sector is found.
 */
class RssiDirectionFinder {

    companion object {
        const val MIN_SAMPLES = 6
        const val MIN_RSSI_SPREAD_DB = 5
        private const val MAX_AGE_MS = 60_000L   // evict samples older than 60 s
        private const val HALF_LIFE_MS = 12_000.0 // recent samples count 2× vs 12 s old

        /** Accept a new sample after this many degrees of azimuth movement. */
        private const val MIN_AZ_DELTA_DEG = 2f
        const val NUM_BINS = 36
    }

    data class Sample(
        val azimuth: Float,
        val elevation: Float,
        val rssi: Int,
        val timestampMs: Long
    )

    private val samples = ArrayDeque<Sample>()

    val sampleCount: Int get() = samples.size
    val maxRssi: Int get() = samples.maxOfOrNull { it.rssi } ?: -120

    val azimuthBins: List<Boolean>
        get() {
            val bins = MutableList(NUM_BINS) { false }
            samples.forEach { s ->
                bins[((s.azimuth / 10f).toInt()).coerceIn(0, NUM_BINS - 1)] = true
            }
            return bins
        }

    val coverage: Float get() = azimuthBins.count { it } / NUM_BINS.toFloat()

    val estimate: Pair<Float, Float>?
        get() {
            if (samples.size < MIN_SAMPLES) return null
            val minRssi = samples.minOf { it.rssi }
            val maxRssi = samples.maxOf { it.rssi }
            if (maxRssi - minRssi < MIN_RSSI_SPREAD_DB) return null

            val now = System.currentTimeMillis()
            var sinSum = 0.0;
            var cosSum = 0.0;
            var elSum = 0.0;
            var wSum = 0.0

            samples.forEach { s ->
                val ageMs = (now - s.timestampMs).coerceAtLeast(0L)
                val timeFactor = exp(-ageMs / HALF_LIFE_MS)          // decay: recent = 1, 12s ago ≈ 0.5
                val rssiFactor = (s.rssi - minRssi + 1.0).pow(3.0)  // cubic: emphasise peak
                val w = rssiFactor * timeFactor
                sinSum += w * sin(Math.toRadians(s.azimuth.toDouble()))
                cosSum += w * cos(Math.toRadians(s.azimuth.toDouble()))
                elSum += w * s.elevation
                wSum += w
            }

            if (wSum == 0.0) return null
            val az = ((Math.toDegrees(atan2(sinSum, cosSum)) + 360.0) % 360.0).toFloat()
            return Pair(az, (elSum / wSum).toFloat())
        }

    val isConfident: Boolean get() = estimate != null && coverage >= 0.25f

    fun addSample(azimuth: Float, elevation: Float, rssi: Int): Boolean {
        val now = System.currentTimeMillis()
        while (samples.isNotEmpty() && now - samples.first().timestampMs > MAX_AGE_MS) {
            samples.removeFirst()
        }
        val lastAz = samples.lastOrNull()?.azimuth
        if (lastAz != null && abs(angleDiff(azimuth, lastAz)) < MIN_AZ_DELTA_DEG) return false
        samples.addLast(Sample(azimuth, elevation, rssi, now))
        return true
    }

    fun clear() = samples.clear()

    private fun angleDiff(a: Float, b: Float): Float =
        (((a - b + 180f + 360f) % 360f) - 180f)
}
