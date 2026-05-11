package com.example.pinpointer.data.repository

import com.example.pinpointer.data.model.TelemetryJson
import com.example.pinpointer.data.remote.SopdetApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.atomic.AtomicBoolean

class TelemetryRepository(private val api: SopdetApi) {
    private val _latestTelemetry = MutableStateFlow<TelemetryJson?>(null)
    val latestTelemetry: StateFlow<TelemetryJson?> = _latestTelemetry.asStateFlow()

    private val _status = MutableStateFlow<com.example.pinpointer.data.model.StatusJson?>(null)
    val status: StateFlow<com.example.pinpointer.data.model.StatusJson?> = _status.asStateFlow()

    private val _history = MutableStateFlow<List<TelemetryJson>>(emptyList())
    val history: StateFlow<List<TelemetryJson>> = _history.asStateFlow()

    private val _isPolling = AtomicBoolean(false)

    suspend fun startPolling() {
        if (_isPolling.getAndSet(true)) return

        while (_isPolling.get()) {
            // ── Status (always poll — uptime/GPS/survey-in work even without packets) ──
            try {
                _status.value = api.getStatus()
            } catch (_: Exception) { /* base station temporarily unreachable */
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

            delay(500)
        }
    }

    fun stopPolling() {
        _isPolling.set(false)
    }

    suspend fun sendEmergencyLocate() = api.sendEmergencyLocate()
    suspend fun stopEmergencyLocate() = api.stopEmergencyLocate()
    suspend fun sendDeployEjection() = api.sendDeployEjection()
    suspend fun requestResurvey() = api.requestResurvey()
    suspend fun setSvinDuration(seconds: Int) = api.setSvinDuration(
        com.example.pinpointer.data.model.SvinConfigBody(seconds)
    )
}
