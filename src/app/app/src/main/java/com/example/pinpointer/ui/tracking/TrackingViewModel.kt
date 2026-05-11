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
    private var lastSeenSeq: Int = -1

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
        lastSeenSeq = -1
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
        val lastMs = telemetry?.receivedAt ?: 0L
        val secondsSince = if (lastMs == 0L) 9999L else (now - lastMs) / 1000L

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
            distanceM = Math.sqrt(hDist * hDist + vDiff * vDiff)
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
            SignalState.GPS_FIX -> {
                rssiDirectionFinder.clear()
                lastSeenSeq = -1
            }

            SignalState.RSSI_ONLY -> {
                if (telemetry != null && telemetry.sequenceNum != lastSeenSeq) {
                    rssiDirectionFinder.addSample(
                        orientation.cameraAzimuth,
                        orientation.cameraElevation,
                        telemetry.rssi
                    )
                    lastSeenSeq = telemetry.sequenceNum
                }
            }

            SignalState.LOST_CONTACT -> Unit
        }

        val rssiEst = rssiDirectionFinder.estimate
        val rssiHasEst = rssiEst != null

        // ── Final target / deviation ───────────────────────────────────────────
        val (finalAz, finalEl) = when {
            distanceIsGps -> Pair(targetAzimuth, targetElevation)
            rssiHasEst -> Pair(rssiEst!!.first.toDouble(), rssiEst.second.toDouble())
            else -> Pair(0.0, 0.0)
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
            isForceRssiMode = forceRssi,
            rssiHasEstimate = rssiHasEst,
            rssiScanCoverage = rssiDirectionFinder.coverage,
            rssiAzimuthBins = rssiDirectionFinder.azimuthBins,
            rssiMaxRssi = rssiDirectionFinder.maxRssi,
            rssiSampleCount = rssiDirectionFinder.sampleCount
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TrackingUiState())
}
