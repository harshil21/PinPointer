package com.example.pinpointer.util

import org.junit.Assert.assertEquals
import org.junit.Test

class CoordinateMathTest {

    @Test
    fun testDistanceCalculation() {
        // Distance between two points (approx 111m per 0.001 degree at equator)
        val lat1 = 0.0
        val lon1 = 0.0
        val lat2 = 0.001
        val lon2 = 0.0
        
        val distance = CoordinateMath.calculateDistance(lat1, lon1, lat2, lon2)
        assertEquals(111.19, distance, 0.1)
    }

    @Test
    fun testBearingCalculation() {
        // Bearing from (0,0) to (0.001, 0) should be 0 degrees (North)
        val lat1 = 0.0
        val lon1 = 0.0
        val lat2 = 0.001
        val lon2 = 0.0
        
        val bearing = CoordinateMath.calculateBearing(lat1, lon1, lat2, lon2)
        assertEquals(0.0, bearing, 0.01)
        
        // Bearing from (0,0) to (0, 0.001) should be 90 degrees (East)
        val bearingEast = CoordinateMath.calculateBearing(0.0, 0.0, 0.0, 0.001)
        assertEquals(90.0, bearingEast, 0.01)
    }

    @Test
    fun testElevationCalculation() {
        // 45 degrees elevation if horizontal distance equals altitude difference
        val distance = 100.0
        val alt1 = 0f
        val alt2 = 100f
        
        val elevation = CoordinateMath.calculateElevation(distance, alt1, alt2)
        assertEquals(45.0, elevation, 0.01)
    }

    @Test
    fun testFriisEquation() {
        // At 100m, freq 915MHz, Pt=20dBm, Gt=2, Gr=8.4
        // Path Loss = 20 log10(100) + 20 log10(915) - 27.55 = 40 + 59.23 - 27.55 = 71.68 dB
        // Pr = Pt + Gt + Gr - L = 20 + 2 + 8.4 - 71.68 = -41.28 dBm
        
        val rssi = -41
        val distance = CoordinateMath.estimateDistanceFriis(rssi, txPowerDbm = 20.0)
        assertEquals(96.8, distance, 1.0)
    }

    @Test
    fun testFriisDefaultTxPowerIs13Dbm() {
        val distance = CoordinateMath.estimateDistanceFriis(-48)
        assertEquals(96.8, distance, 1.0)
    }
}
