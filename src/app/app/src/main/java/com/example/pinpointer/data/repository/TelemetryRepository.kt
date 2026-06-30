package com.example.pinpointer.data.repository

import com.example.pinpointer.data.model.TelemetryJson
import com.example.pinpointer.data.remote.SopdetApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.InetAddress
import java.util.concurrent.atomic.AtomicBoolean

enum class BaseStationConnectionState {
    DISCONNECTED,
    ONLINE,
    SOPDET_UNAVAILABLE,
    HOST_UNREACHABLE
}

class TelemetryRepository(
    private val api: SopdetApi,
    private val host: String
) {
    private val _latestTelemetry = MutableStateFlow<TelemetryJson?>(null)
    val latestTelemetry: StateFlow<TelemetryJson?> = _latestTelemetry.asStateFlow()

    private val _status = MutableStateFlow<com.example.pinpointer.data.model.StatusJson?>(null)
    val status: StateFlow<com.example.pinpointer.data.model.StatusJson?> = _status.asStateFlow()

    private val _history = MutableStateFlow<List<TelemetryJson>>(emptyList())
    val history: StateFlow<List<TelemetryJson>> = _history.asStateFlow()

    private val _connectionState = MutableStateFlow(BaseStationConnectionState.DISCONNECTED)
    val connectionState: StateFlow<BaseStationConnectionState> = _connectionState.asStateFlow()

    private val _isPolling = AtomicBoolean(false)

    suspend fun startPolling() {
        if (_isPolling.getAndSet(true)) return

        while (_isPolling.get()) {
            // ── Status (always poll — uptime/GPS/survey-in work even without packets) ──
            try {
                _status.value = api.getStatus()
                _connectionState.value = BaseStationConnectionState.ONLINE
            } catch (_: Exception) {
                _connectionState.value = if (isHostReachable()) {
                    BaseStationConnectionState.SOPDET_UNAVAILABLE
                } else {
                    BaseStationConnectionState.HOST_UNREACHABLE
                }
            }

            // ── Latest telemetry (may be 204 if rocket hasn't transmitted yet) ──
            try {
                val telemetry = api.getLatestTelemetry()
                _latestTelemetry.value = telemetry

                val current = _history.value.toMutableList()
                if (current.isEmpty() || current.last().sequenceNum != telemetry.sequenceNum) {
                    current.add(telemetry)
                    if (current.size > 1_000) current.removeAt(0)
                    _history.value = current.toList()
                }
            } catch (_: Exception) { /* no telemetry yet or 204 response */
            }

            // 250 ms keeps the UI responsive (base-station GPS / SVIN updates
            // show up within ~half the GPS 1 Hz cadence) without flooding the
            // sopdet HTTP server.
            delay(250)
        }
    }

    fun stopPolling() {
        _isPolling.set(false)
    }

    private suspend fun isHostReachable(): Boolean = withContext(Dispatchers.IO) {
        try {
            val rawHost = host.substringBefore(':')
            InetAddress.getByName(rawHost).isReachable(800)
        } catch (_: Exception) {
            false
        }
    }

    suspend fun sendEmergencyLocate() = api.sendEmergencyLocate()
    suspend fun stopEmergencyLocate() = api.stopEmergencyLocate()
    suspend fun sendDeployEjection() = api.sendDeployEjection()
    suspend fun zeroAltitude() = api.zeroAltitude()
    suspend fun setTxPower(dbm: Int) = api.setTxPower(
        com.example.pinpointer.data.model.TxPowerBody(dbm)
    )
    suspend fun requestResurvey() = api.requestResurvey()
    suspend fun enableDebugTelemetry() = api.enableDebugTelemetry()
    suspend fun disableDebugTelemetry() = api.disableDebugTelemetry()
    suspend fun setSvinDuration(seconds: Int) = api.setSvinDuration(
        com.example.pinpointer.data.model.SvinConfigBody(seconds)
    )
}
