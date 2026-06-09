package com.example.pinpointer.ui.tracking

import com.example.pinpointer.data.model.ConstellationSnrJson
import com.example.pinpointer.data.model.TelemetryJson
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RocketSnrSummaryTest {

    private fun telemetry(snr: Int = 0): TelemetryJson = TelemetryJson(
        receivedAt = 0L, sequenceNum = 0, timestampMs = 0L,
        altitudeM = 0f, velocityMps = 0f, accelZGs = 0f,
        gpsLat = 0.0, gpsLon = 0.0, gpsAltM = 0f,
        rtkFix = "NoFix",
        pyroDeployed = false, pyroContinuity = false,
        flightState = 0, rssi = -100, snr = 0f,
        gpsSNR = snr
    )

    @Test
    fun `falls back to telemetry average when debug is null`() {
        val s = rocketSnrSummary(telemetry(snr = 32), debugSnr = null)
        assertEquals(32, s.displayValue)
        assertEquals(RocketSnrSummary.Source.TELEMETRY_AVERAGE, s.source)
        assertTrue(s.hasValue)
        assertNull(s.perConstellation)
    }

    @Test
    fun `prefers debug average when present`() {
        val debug = ConstellationSnrJson(gps = 30, glonass = 28, average = 29)
        val s = rocketSnrSummary(telemetry(snr = 10), debug)
        assertEquals(29, s.displayValue)
        assertEquals(RocketSnrSummary.Source.DEBUG_AVERAGE, s.source)
    }

    @Test
    fun `uses per-constellation max when server average is zero but values exist`() {
        // Server-computed average can be zero before all constellations report,
        // but individual constellations may already have non-zero SNR.
        val debug = ConstellationSnrJson(gps = 22, glonass = 0, galileo = 31, average = 0)
        val s = rocketSnrSummary(telemetry(snr = 0), debug)
        assertEquals(31, s.displayValue)
        assertEquals(RocketSnrSummary.Source.DEBUG_MAX, s.source)
        assertTrue(s.hasValue)
        assertTrue(s.hasDetails)
    }

    @Test
    fun `no value reported when nothing is populated`() {
        val s = rocketSnrSummary(telemetry(snr = 0), debugSnr = null)
        assertEquals(0, s.displayValue)
        assertEquals(RocketSnrSummary.Source.NONE, s.source)
        assertFalse(s.hasValue)
    }

    @Test
    fun `null telemetry does not crash and reports zero when no debug data`() {
        val s = rocketSnrSummary(telemetry = null, debugSnr = null)
        assertEquals(0, s.displayValue)
        assertEquals(RocketSnrSummary.Source.NONE, s.source)
    }

    @Test
    fun `null telemetry with non-empty debug still produces a value`() {
        val debug = ConstellationSnrJson(galileo = 27, average = 27)
        val s = rocketSnrSummary(telemetry = null, debugSnr = debug)
        assertEquals(27, s.displayValue)
        assertEquals(RocketSnrSummary.Source.DEBUG_AVERAGE, s.source)
    }
}
