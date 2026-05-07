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

enum class SignalState {
    GPS_FIX,       // Green  — GPS + RTK active
    RSSI_ONLY,     // Amber  — estimating via Friis / RSSI scan
    LOST_CONTACT   // Red    — no packets recently
}

data class TrackingUiState(
    val telemetry: TelemetryJson? = null,
    val status: StatusJson? = null,
    val orientation: OrientationData = OrientationData(),
    val signalState: SignalState = SignalState.LOST_CONTACT,
    val secondsSinceLastContact: Long = 9999L,
    /** 3-D slant distance to rocket in metres. */
    val distanceM: Double = 0.0,
    /** Compass bearing of the rocket from the base station, degrees [0, 360). */
    val targetAzimuth: Double = 0.0,
    /** Elevation angle of the rocket from the base station, degrees. */
    val targetElevation: Double = 0.0,
    /**
     * Signed deviation: positive = rotate phone right (clockwise) to aim.
     * In (-180, 180].  Valid when [distanceIsGps] or [rssiHasEstimate].
     */
    val deviationAzimuth: Double = 0.0,
    /** Signed deviation: positive = tilt phone up to aim. */
    val deviationElevation: Double = 0.0,
    /** Whether the distance/direction is GPS-derived (true) or RSSI-estimated (false). */
    val distanceIsGps: Boolean = false,

    // ── RSSI direction-finding state ──────────────────────────────────────────
    /** The finder currently has a statistically reliable direction estimate. */
    val rssiHasEstimate: Boolean = false,
    /** Fraction of the full 360° azimuth that has been sampled so far, [0, 1]. */
    val rssiScanCoverage: Float = 0f,
    /**
     * 36-element list.  Element `i` is true if the azimuth sector
     * [i×10°, (i+1)×10°) has been sampled.
     */
    val rssiAzimuthBins: List<Boolean> = List(36) { false },
    /** Peak RSSI seen across all current RSSI-scan samples. */
    val rssiMaxRssi: Int = -120,
    /** Total number of RSSI-scan samples accumulated. */
    val rssiSampleCount: Int = 0
)

class TrackingViewModel(
    val repository: TelemetryRepository,
    private val orientationTracker: OrientationTracker
) : ViewModel() {

    private val rssiDirectionFinder = RssiDirectionFinder()

    /** Last telemetry sequence number seen — used to gate RSSI sample collection. */
    private var lastSeenSeq: Int = -1

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
        orientationTracker.orientation
    ) { telemetry, status, orientation ->

        // ── Signal state ───────────────────────────────────────────────────────
        val now = System.currentTimeMillis()
        val lastContactMs = telemetry?.receivedAt ?: 0L
        val secondsSince = if (lastContactMs == 0L) 9999L else (now - lastContactMs) / 1000L

        val signalState = when {
            telemetry == null || secondsSince > 5L -> SignalState.LOST_CONTACT
            telemetry.rtkFix !in listOf("NoFix", "DeadReckoning") && status?.gpsFix != null ->
                SignalState.GPS_FIX

            else -> SignalState.RSSI_ONLY
        }

        // ── GPS-based target ───────────────────────────────────────────────────
        var distanceM = 0.0
        var targetAzimuth = 0.0
        var targetElevation = 0.0
        var distanceIsGps = false

        if (telemetry != null && status?.gpsFix != null) {
            val horizDist = CoordinateMath.calculateDistance(
                status.gpsFix.latitude, status.gpsFix.longitude,
                telemetry.gpsLat, telemetry.gpsLon
            )
            val vertDiff = (telemetry.gpsAltM - status.gpsFix.altitudeM).toDouble()
            distanceM = Math.sqrt(horizDist * horizDist + vertDiff * vertDiff)
            targetAzimuth = CoordinateMath.calculateBearing(
                status.gpsFix.latitude, status.gpsFix.longitude,
                telemetry.gpsLat, telemetry.gpsLon
            )
            targetElevation = CoordinateMath.calculateElevation(
                horizDist, status.gpsFix.altitudeM, telemetry.gpsAltM
            )
            distanceIsGps = true
        } else if (telemetry != null) {
            distanceM = CoordinateMath.estimateDistanceFriis(telemetry.rssi)
        }

        // ── RSSI direction-finder ─────────────────────────────────────────────
        when (signalState) {
            SignalState.GPS_FIX -> {
                // GPS available — clear stale scan data
                rssiDirectionFinder.clear()
                lastSeenSeq = -1
            }

            SignalState.RSSI_ONLY -> {
                // Only add a sample once per newly-received packet (not every orientation tick)
                if (telemetry != null && telemetry.sequenceNum != lastSeenSeq) {
                    rssiDirectionFinder.addSample(
                        azimuth = orientation.cameraAzimuth,
                        elevation = orientation.cameraElevation,
                        rssi = telemetry.rssi
                    )
                    lastSeenSeq = telemetry.sequenceNum
                }
            }

            SignalState.LOST_CONTACT -> { /* do nothing */
            }
        }

        val rssiEstimate = rssiDirectionFinder.estimate
        val rssiHasEstimate = rssiEstimate != null

        // ── Determine final target for deviation display ───────────────────────
        val (finalTargetAz, finalTargetEl) = when {
            distanceIsGps -> Pair(targetAzimuth, targetElevation)
            rssiHasEstimate -> Pair(
                rssiEstimate!!.first.toDouble(),
                rssiEstimate.second.toDouble()
            )

            else -> Pair(0.0, 0.0)
        }

        val hasTarget = distanceIsGps || rssiHasEstimate
        val deviationAzimuth = if (hasTarget)
            CoordinateMath.angleDifference(finalTargetAz, orientation.cameraAzimuth.toDouble())
        else 0.0

        val deviationElevation = if (hasTarget)
            finalTargetEl - orientation.cameraElevation.toDouble()
        else 0.0

        TrackingUiState(
            telemetry = telemetry,
            status = status,
            orientation = orientation,
            signalState = signalState,
            secondsSinceLastContact = secondsSince,
            distanceM = distanceM,
            targetAzimuth = finalTargetAz,
            targetElevation = finalTargetEl,
            deviationAzimuth = deviationAzimuth,
            deviationElevation = deviationElevation,
            distanceIsGps = distanceIsGps,
            rssiHasEstimate = rssiHasEstimate,
            rssiScanCoverage = rssiDirectionFinder.coverage,
            rssiAzimuthBins = rssiDirectionFinder.azimuthBins,
            rssiMaxRssi = rssiDirectionFinder.maxRssi,
            rssiSampleCount = rssiDirectionFinder.sampleCount
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TrackingUiState())
}
