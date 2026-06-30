package com.example.pinpointer.ui.tracking

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.example.pinpointer.data.model.StatusJson
import com.example.pinpointer.data.model.TelemetryJson
import com.example.pinpointer.data.repository.TelemetryRepository
import com.example.pinpointer.util.CoordinateMath
import com.example.pinpointer.util.OrientationData
import com.example.pinpointer.util.OrientationTracker
import com.example.pinpointer.util.RssiDirectionFinder
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

enum class SignalState {
    GPS_FIX,       // GPS or RTK fix active
    RSSI_ONLY,     // Estimating via Friis / RSSI scan
    LOST_CONTACT   // No packets recently
}

data class TrackingUiState(
    val telemetry: TelemetryJson? = null,
    val status: StatusJson? = null,
    val orientation: OrientationData = OrientationData(),
    val signalState: SignalState = SignalState.LOST_CONTACT,
    val secondsSinceLastContact: Long = 9999L,
    val distanceM: Double = 0.0,
    val targetAzimuth: Double = 0.0,
    val targetElevation: Double = 0.0,
    /** Positive = rotate phone right (clockwise). In (-180, 180]. */
    val deviationAzimuth: Double = 0.0,
    /** Positive = tilt phone up. */
    val deviationElevation: Double = 0.0,
    val distanceIsGps: Boolean = false,
    val targetSource: String = "NO TARGET",
    /** Settings toggle: use RSSI scan even when GPS is available. */
    val isForceRssiMode: Boolean = false,
    // ── RSSI direction-finding ────────────────────────────────────────────────
    val rssiHasEstimate: Boolean = false,
    val rssiScanCoverage: Float = 0f,
    val rssiAzimuthBins: List<Boolean> = List(36) { false },
    val rssiMaxRssi: Int = -120,
    val rssiSampleCount: Int = 0
)

class TrackingViewModel(
    val repository: TelemetryRepository,
    private val orientationTracker: OrientationTracker
) : ViewModel() {

    private val rssiDirectionFinder = RssiDirectionFinder()
    private var lastContactSeq: Int = -1
    private var lastRssiSampleSeq: Int = -1

    /**
     * Local phone-side timestamp (ms) of the last time we observed a new
     * sequence number from the rocket.  This intentionally does NOT use
     * [TelemetryJson.receivedAt] because Sopdet runs on a Raspberry Pi whose
     * system clock is often un-synced at a launch site (no internet → no NTP,
     * no GPS fix → no GPSDO).  Using the server timestamp would make
     * [secondsSince] astronomically large, permanently forcing LOST_CONTACT
     * even while packets are flowing.
     */
    private var localLastContactMs: Long = 0L

    private val _forceRssiOnly = MutableStateFlow(false)
    fun setForceRssiOnly(value: Boolean) {
        _forceRssiOnly.value = value
    }

    /**
     * Discard all RSSI samples and reset the direction finder. Used by the
     * "Recalibrate" button on the Tracking screen — only effective while we
     * are in RSSI-only tracking; in any other mode the call is a no-op.
     */
    fun recalibrateRssiTracking() {
        rssiDirectionFinder.clear()
        lastRssiSampleSeq = -1
    }

    init {
        viewModelScope.launch { repository.startPolling() }
        orientationTracker.start()
    }

    override fun onCleared() {
        super.onCleared()
        repository.stopPolling()
        orientationTracker.stop()
    }

    val uiState: StateFlow<TrackingUiState> = combine(
        repository.latestTelemetry,
        repository.status,
        orientationTracker.orientation,
        _forceRssiOnly
    ) { telemetry, status, orientation, forceRssi ->

        // ── Signal state ───────────────────────────────────────────────────────
        val now = System.currentTimeMillis()
        // Advance local contact clock on every NEW sequence number so the
        // freshness check is immune to clock skew between Sopdet and the phone.
        val hasNewTelemetry = telemetry != null && telemetry.sequenceNum != lastContactSeq
        if (hasNewTelemetry) {
            localLastContactMs = now
            lastContactSeq = telemetry.sequenceNum
        }
        val secondsSince =
            if (localLastContactMs == 0L) 9999L
            else (now - localLastContactMs) / 1000L

        val signalState = when {
            telemetry == null || secondsSince > 5L -> SignalState.LOST_CONTACT
            !forceRssi && telemetry.rtkFix !in listOf("NoFix", "DeadReckoning")
                    && status?.gpsFix != null -> SignalState.GPS_FIX

            else -> SignalState.RSSI_ONLY
        }

        // ── GPS target ─────────────────────────────────────────────────────────
        var distanceM = 0.0
        var targetAzimuth = 0.0
        var targetElevation = 0.0
        var distanceIsGps = false

        if (!forceRssi && telemetry != null && status?.gpsFix != null
            && telemetry.gpsLat != 0.0 && telemetry.gpsLon != 0.0
            && status.gpsFix.latitude != 0.0
        ) {
            val hDist = CoordinateMath.calculateDistance(
                status.gpsFix.latitude, status.gpsFix.longitude,
                telemetry.gpsLat, telemetry.gpsLon
            )
            val vDiff = (telemetry.gpsAltM - status.gpsFix.altitudeM).toDouble()
            distanceM = sqrt(hDist * hDist + vDiff * vDiff)
            targetAzimuth = CoordinateMath.calculateBearing(
                status.gpsFix.latitude, status.gpsFix.longitude,
                telemetry.gpsLat, telemetry.gpsLon
            )
            targetElevation = CoordinateMath.calculateElevation(
                hDist, status.gpsFix.altitudeM, telemetry.gpsAltM
            )
            distanceIsGps = true
        } else if (telemetry != null) {
            distanceM = CoordinateMath.estimateDistanceFriis(telemetry.rssi)
        }

        // ── RSSI finder ────────────────────────────────────────────────────────
        when (signalState) {
            SignalState.GPS_FIX,
            SignalState.RSSI_ONLY -> {
                if (telemetry != null && telemetry.sequenceNum != lastRssiSampleSeq) {
                    rssiDirectionFinder.addSample(
                        orientation.cameraAzimuth,
                        orientation.cameraElevation,
                        telemetry.rssi
                    )
                    lastRssiSampleSeq = telemetry.sequenceNum
                }
            }

            SignalState.LOST_CONTACT -> Unit
        }

        val rssiEst = rssiDirectionFinder.estimate
        val rssiHasEst = rssiEst != null

        // ── Final target / deviation ───────────────────────────────────────────
        val fusionWeight = if (distanceIsGps && rssiHasEst) {
            (rssiDirectionFinder.coverage.coerceIn(0f, 0.6f) * 0.55f).toDouble()
        } else {
            0.0
        }
        val (finalAz, finalEl, targetSource) = if (rssiEst != null) {
            when {
                distanceIsGps && fusionWeight > 0.05 -> Triple(
                    blendAngles(targetAzimuth, rssiEst.first.toDouble(), fusionWeight),
                    targetElevation * (1.0 - fusionWeight) + rssiEst.second.toDouble() * fusionWeight,
                    "GPS + RSSI"
                )

                distanceIsGps -> Triple(targetAzimuth, targetElevation, "GPS")
                else -> Triple(rssiEst.first.toDouble(), rssiEst.second.toDouble(), "RSSI")
            }
        } else if (distanceIsGps) {
            Triple(targetAzimuth, targetElevation, "GPS")
        } else {
            Triple(0.0, 0.0, "NO TARGET")
        }
        val hasTarget = distanceIsGps || rssiHasEst
        val devAz = if (hasTarget)
            CoordinateMath.angleDifference(finalAz, orientation.cameraAzimuth.toDouble()) else 0.0
        val devEl = if (hasTarget) finalEl - orientation.cameraElevation.toDouble() else 0.0

        TrackingUiState(
            telemetry = telemetry,
            status = status,
            orientation = orientation,
            signalState = signalState,
            secondsSinceLastContact = secondsSince,
            distanceM = distanceM,
            targetAzimuth = finalAz,
            targetElevation = finalEl,
            deviationAzimuth = devAz,
            deviationElevation = devEl,
            distanceIsGps = distanceIsGps,
            targetSource = targetSource,
            isForceRssiMode = forceRssi,
            rssiHasEstimate = rssiHasEst,
            rssiScanCoverage = rssiDirectionFinder.coverage,
            rssiAzimuthBins = rssiDirectionFinder.azimuthBins,
            rssiMaxRssi = rssiDirectionFinder.maxRssi,
            rssiSampleCount = rssiDirectionFinder.sampleCount
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TrackingUiState())
}

private fun blendAngles(primaryDeg: Double, secondaryDeg: Double, secondaryWeight: Double): Double {
    val primaryWeight = 1.0 - secondaryWeight.coerceIn(0.0, 1.0)
    val a = Math.toRadians(primaryDeg)
    val b = Math.toRadians(secondaryDeg)
    val x = primaryWeight * cos(a) + secondaryWeight * cos(b)
    val y = primaryWeight * sin(a) + secondaryWeight * sin(b)
    return (Math.toDegrees(atan2(y, x)) + 360.0) % 360.0
}
