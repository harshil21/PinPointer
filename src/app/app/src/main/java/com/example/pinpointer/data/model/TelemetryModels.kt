package com.example.pinpointer.data.model

import com.google.gson.annotations.SerializedName

data class TelemetryJson(
    @SerializedName("received_at") val receivedAt: Long,
    @SerializedName("sequence_num") val sequenceNum: Int,
    @SerializedName("timestamp_ms") val timestampMs: Long,
    @SerializedName("altitude_m") val altitudeM: Float,
    @SerializedName("velocity_mps") val velocityMps: Float,
    @SerializedName("accel_z_gs") val accelZGs: Float,
    @SerializedName("gps_lat") val gpsLat: Double,
    @SerializedName("gps_lon") val gpsLon: Double,
    @SerializedName("gps_alt_m") val gpsAltM: Float,
    @SerializedName("rtk_fix") val rtkFix: String,
    @SerializedName("pyro_deployed") val pyroDeployed: Boolean,
    @SerializedName("pyro_continuity") val pyroContinuity: Boolean,
    @SerializedName("flight_state") val flightState: Int,
    @SerializedName("rssi") val rssi: Int,
    @SerializedName("snr") val snr: Float,
    /** Average GPS SNR on the rocket in dB-Hz (from NMEA GSV via downlink). */
    @SerializedName("gps_snr") val gpsSNR: Int = 0,
    @SerializedName("hdop") val hdop: Float = 0f,
    @SerializedName("satellites_used") val satellitesUsed: Int = 0
)

data class StatusJson(
    @SerializedName("uptime_seconds") val uptimeSeconds: Long,
    @SerializedName("svin_complete") val svinComplete: Boolean,
    @SerializedName("svin_active") val svinActive: Boolean,
    @SerializedName("gps_fix") val gpsFix: GpsFixJson?,
    @SerializedName("telemetry_count") val telemetryCount: Int,
    @SerializedName("last_downlink_rssi") val lastDownlinkRssi: Int?,
    /** Per-constellation SNR at the base station. */
    @SerializedName("gps_snr") val gpsSNR: ConstellationSnrJson = ConstellationSnrJson(),
    @SerializedName("svin_duration_s") val svinDurationS: Int = 120
)

data class GpsFixJson(
    @SerializedName("latitude") val latitude: Double,
    @SerializedName("longitude") val longitude: Double,
    @SerializedName("altitude_m") val altitudeM: Float,
    @SerializedName("fix_quality") val fixQuality: String,
    @SerializedName("satellites_used") val satellitesUsed: Int,
    @SerializedName("hdop") val hdop: Float
)

/**
 * Per-constellation GPS SNR snapshot from the base station.
 * Each field is the average SNR (dB-Hz) for that constellation's tracked
 * satellites; 0 means no data received yet.
 */
data class ConstellationSnrJson(
    @SerializedName("gps") val gps: Int = 0,  // GP – GPS / NAVSTAR
    @SerializedName("glonass") val glonass: Int = 0,  // GL – GLONASS
    @SerializedName("galileo") val galileo: Int = 0,  // GA – Galileo
    @SerializedName("beidou") val beidou: Int = 0,  // GB – BeiDou
    @SerializedName("qzss") val qzss: Int = 0,  // GQ – QZSS
    /** Average of GPS + GLONASS + Galileo + BeiDou, non-zero only (server-computed). */
    @SerializedName("average") val average: Int = 0
) {
    val hasData: Boolean get() = average > 0

    /** Non-zero constellation entries as a display map. */
    fun activeConstellations(): Map<String, Int> = buildMap {
        if (gps > 0) put("GPS", gps)
        if (glonass > 0) put("GLONASS", glonass)
        if (galileo > 0) put("Galileo", galileo)
        if (beidou > 0) put("BeiDou", beidou)
        if (qzss > 0) put("QZSS", qzss)
    }
}

data class CommandResponse(
    @SerializedName("status") val status: String,
    @SerializedName("command") val command: String
)

data class SvinConfigBody(
    @SerializedName("duration_s") val durationS: Int
)
