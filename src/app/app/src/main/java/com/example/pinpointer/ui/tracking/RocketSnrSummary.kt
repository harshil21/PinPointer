package com.example.pinpointer.ui.tracking

import com.example.pinpointer.data.model.ConstellationSnrJson
import com.example.pinpointer.data.model.TelemetryJson

/**
 * Summary of the rocket's GPS SNR for the Tracking-screen tile.
 *
 * The rocket exposes SNR through two channels:
 * 1. `TelemetryJson.gpsSNR` — average GPS SNR byte in the regular downlink packet.
 *    Only populated once GSV sentences arrive at the rocket's GPS — frequently
 *    zero in practice, even when there is a position fix.
 * 2. `StatusJson.rocketDebugSnr` — per-constellation SNR from the debug-mode
 *    downlink, computed by the base station.
 *
 * We prefer the richer debug snapshot when available, and fall back to the
 * average byte otherwise.
 */
data class RocketSnrSummary(
    val displayValue: Int,
    val source: Source,
    val perConstellation: ConstellationSnrJson?
) {
    enum class Source { DEBUG_AVERAGE, DEBUG_MAX, TELEMETRY_AVERAGE, NONE }

    val hasValue: Boolean get() = displayValue > 0
    val hasDetails: Boolean get() = perConstellation != null
}

fun rocketSnrSummary(
    telemetry: TelemetryJson?,
    debugSnr: ConstellationSnrJson?
): RocketSnrSummary {
    // 1. Prefer debug-mode per-constellation average when present.
    if (debugSnr != null) {
        if (debugSnr.average > 0) {
            return RocketSnrSummary(debugSnr.average, RocketSnrSummary.Source.DEBUG_AVERAGE, debugSnr)
        }
        // The server-computed average can lag the per-constellation values
        // (it covers only those constellations contributing > 0). Fall back
        // to the strongest available per-constellation reading so the tile
        // updates as soon as a single GSV burst lands.
        val maxPer = maxOf(
            debugSnr.gps, debugSnr.glonass, debugSnr.galileo, debugSnr.beidou, debugSnr.qzss
        )
        if (maxPer > 0) {
            return RocketSnrSummary(maxPer, RocketSnrSummary.Source.DEBUG_MAX, debugSnr)
        }
    }
    // 2. Plain telemetry average byte.
    val avg = telemetry?.gpsSNR ?: 0
    if (avg > 0) {
        return RocketSnrSummary(avg, RocketSnrSummary.Source.TELEMETRY_AVERAGE, debugSnr)
    }
    return RocketSnrSummary(0, RocketSnrSummary.Source.NONE, debugSnr)
}
