package com.example.pinpointer.util

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlin.math.*

/**
 * Tracks phone orientation using the game rotation vector sensor.
 *
 * Key properties:
 * - [cameraAzimuth]: the compass bearing the rear camera points toward, in [0, 360) degrees.
 *   0 = North, 90 = East, 180 = South, 270 = West.
 * - [cameraElevation]: angle above horizontal of the rear camera, in [-90, 90] degrees.
 *   0 = horizontal, 90 = straight up, -90 = straight down.
 *
 * Because the yagi is fixed to the phone with its axis along the camera direction,
 * these two values fully describe where the antenna is pointing.
 */
data class OrientationData(
    val rotationMatrix: FloatArray = FloatArray(9) { if (it == 0 || it == 4 || it == 8) 1f else 0f },
    /** Screen-Y-axis azimuth (from getOrientation). Kept for legacy use. */
    val azimuth: Float = 0f,
    val pitch: Float = 0f,
    val roll: Float = 0f,
    /** Bearing of the rear camera / yagi axis, degrees [0, 360). */
    val cameraAzimuth: Float = 0f,
    /** Elevation of the rear camera / yagi axis, degrees [-90, 90]. */
    val cameraElevation: Float = 0f
) {
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is OrientationData) return false
        return cameraAzimuth == other.cameraAzimuth && cameraElevation == other.cameraElevation
    }

    override fun hashCode() = 31 * cameraAzimuth.hashCode() + cameraElevation.hashCode()
}

class OrientationTracker(context: Context) : SensorEventListener {

    private val sensorManager = context.getSystemService(Context.SENSOR_SERVICE) as SensorManager

    // Prefer ROTATION_VECTOR (magnetic north reference) so azimuth is absolute.
    private val rotationSensor = sensorManager.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)

    private val _orientation = MutableStateFlow(OrientationData())
    val orientation: StateFlow<OrientationData> = _orientation.asStateFlow()

    fun start() {
        rotationSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_GAME)
        }
    }

    fun stop() {
        sensorManager.unregisterListener(this)
    }

    override fun onSensorChanged(event: SensorEvent?) {
        if (event?.sensor?.type != Sensor.TYPE_ROTATION_VECTOR) return

        val R = FloatArray(9)
        SensorManager.getRotationMatrixFromVector(R, event.values)

        // R maps device coordinates → world coordinates.
        // World: X = East, Y = North, Z = Up.
        // Device: X = screen right, Y = screen top, Z = out of screen (toward user's face).
        //
        // The rear camera lies in the -device_Z direction.
        // Its world direction = R * [0, 0, -1]ᵀ = (-R[2], -R[5], -R[8]).
        val camEast = -R[2]
        val camNorth = -R[5]
        val camUp = -R[8].coerceIn(-1f, 1f)

        val rawAzDeg = Math.toDegrees(atan2(camEast.toDouble(), camNorth.toDouble())).toFloat()
        val cameraAzimuth = (rawAzDeg + 360f) % 360f
        val cameraElevation = Math.toDegrees(asin(camUp.toDouble())).toFloat()

        // Legacy Euler angles (getOrientation)
        val euler = FloatArray(3)
        SensorManager.getOrientation(R, euler)

        _orientation.value = OrientationData(
            rotationMatrix = R,
            azimuth = Math.toDegrees(euler[0].toDouble()).toFloat(),
            pitch = Math.toDegrees(euler[1].toDouble()).toFloat(),
            roll = Math.toDegrees(euler[2].toDouble()).toFloat(),
            cameraAzimuth = cameraAzimuth,
            cameraElevation = cameraElevation
        )
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {}
}

// ── Math utilities ────────────────────────────────────────────────────────────

object CoordinateMath {

    /** Haversine distance in metres. */
    fun calculateDistance(lat1: Double, lon1: Double, lat2: Double, lon2: Double): Double {
        val r = 6_371_000.0
        val dLat = Math.toRadians(lat2 - lat1)
        val dLon = Math.toRadians(lon2 - lon1)
        val a = sin(dLat / 2).pow(2) +
                cos(Math.toRadians(lat1)) * cos(Math.toRadians(lat2)) * sin(dLon / 2).pow(2)
        return r * 2 * atan2(sqrt(a), sqrt(1 - a))
    }

    /** True bearing from point 1 → point 2, degrees [0, 360). */
    fun calculateBearing(lat1: Double, lon1: Double, lat2: Double, lon2: Double): Double {
        val dLon = Math.toRadians(lon2 - lon1)
        val y = sin(dLon) * cos(Math.toRadians(lat2))
        val x = cos(Math.toRadians(lat1)) * sin(Math.toRadians(lat2)) -
                sin(Math.toRadians(lat1)) * cos(Math.toRadians(lat2)) * cos(dLon)
        return (Math.toDegrees(atan2(y, x)) + 360) % 360
    }

    /** Elevation angle in degrees. Positive = target is above the observer. */
    fun calculateElevation(horizontalDist: Double, observerAlt: Float, targetAlt: Float): Double {
        if (horizontalDist < 1.0) return 90.0
        return Math.toDegrees(atan2((targetAlt - observerAlt).toDouble(), horizontalDist))
    }

    /**
     * Shortest signed angular difference from [from] to [to], in (-180, 180].
     * Positive = turn clockwise / turn right.
     */
    fun angleDifference(to: Double, from: Double): Double {
        return ((to - from + 540) % 360) - 180
    }

    /**
     * Estimate slant distance using the Friis transmission equation.
     * Defaults: TX power 20 dBm, yagi gain 8.4 dBi, PCB antenna gain 2 dBi.
     */
    fun estimateDistanceFriis(
        rssi: Int,
        txPowerDbm: Double = 20.0,
        freqMhz: Double = 915.0,
        gTxDbi: Double = 2.0,
        gRxDbi: Double = 8.4
    ): Double {
        // Friis: RSSI = Pt + Gt + Gr - FSPL
        // FSPL(dB) = 20*log10(d) + 20*log10(f) + 20*log10(4π/c)
        //          ≈ 20*log10(d) + 20*log10(f_MHz) - 27.55
        val pathLoss = txPowerDbm + gTxDbi + gRxDbi - rssi
        val logD = (pathLoss - 20 * log10(freqMhz) + 27.55) / 20.0
        return 10.0.pow(logD).coerceIn(0.0, 50_000.0)
    }
}
