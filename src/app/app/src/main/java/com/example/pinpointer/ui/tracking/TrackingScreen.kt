package com.example.pinpointer.ui.tracking

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.FlightTakeoff
import androidx.compose.material.icons.rounded.GpsFixed
import androidx.compose.material.icons.rounded.GpsOff
import androidx.compose.material.icons.rounded.SignalCellularOff
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.example.pinpointer.ui.theme.DotBad
import com.example.pinpointer.ui.theme.DotGood
import com.example.pinpointer.ui.theme.DotOk
import com.example.pinpointer.ui.theme.FlightStateCoast
import com.example.pinpointer.ui.theme.FlightStateFreefall
import com.example.pinpointer.ui.theme.FlightStateLanded
import com.example.pinpointer.ui.theme.FlightStateMotorBurn
import com.example.pinpointer.ui.theme.FlightStateStandby
import com.example.pinpointer.ui.theme.RadarBackground
import com.example.pinpointer.ui.theme.RadarGrid
import com.example.pinpointer.ui.theme.DataTextStyle
import com.example.pinpointer.ui.theme.DataTextStyleLarge
import com.example.pinpointer.ui.theme.DataTextStyleSmall
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

// ── Helpers ───────────────────────────────────────────────────────────────────

private fun flightStateName(s: Int) = when (s) {
    0 -> "Standby"
    1 -> "Motor Burn"
    2 -> "Coast"
    3 -> "Freefall"
    4 -> "Landed"
    else -> "Unknown"
}

private fun flightStateColor(s: Int) = when (s) {
    0 -> FlightStateStandby
    1 -> FlightStateMotorBurn
    2 -> FlightStateCoast
    3 -> FlightStateFreefall
    4 -> FlightStateLanded
    else -> FlightStateStandby
}

// ── Root screen ───────────────────────────────────────────────────────────────

@Composable
fun TrackingScreen(viewModel: TrackingViewModel) {
    val uiState by viewModel.uiState.collectAsState()

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        Column(modifier = Modifier.fillMaxSize()) {
            // ── TOP: scrollable telemetry section ──────────────────────────────
            Column(
                modifier = Modifier
                    .weight(0.52f)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                StatusRow(uiState)
                TelemetryMetrics(uiState)
            }

            // ── BOTTOM: fixed antenna pointing section ─────────────────────────
            AntennaPointingSection(
                modifier = Modifier
                    .weight(0.48f)
                    .fillMaxWidth(),
                uiState = uiState
            )
        }
    }
}

// ── Status row (signal + flight state) ───────────────────────────────────────

@Composable
private fun StatusRow(state: TrackingUiState) {
    val signalColor by animateColorAsState(
        targetValue = when (state.signalState) {
            SignalState.GPS_FIX -> MaterialTheme.colorScheme.primary
            SignalState.RSSI_ONLY -> MaterialTheme.colorScheme.tertiary
            SignalState.LOST_CONTACT -> MaterialTheme.colorScheme.error
        },
        label = "signalColor"
    )

    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        // Signal state pill
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(50))
                .background(signalColor.copy(alpha = 0.15f))
                .padding(horizontal = 12.dp, vertical = 6.dp)
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                Icon(
                    imageVector = when (state.signalState) {
                        SignalState.GPS_FIX -> Icons.Rounded.GpsFixed
                        SignalState.RSSI_ONLY -> Icons.Rounded.GpsOff
                        SignalState.LOST_CONTACT -> Icons.Rounded.SignalCellularOff
                    },
                    contentDescription = null,
                    tint = signalColor,
                    modifier = Modifier.size(14.dp)
                )
                Text(
                    text = when (state.signalState) {
                        SignalState.GPS_FIX -> "GPS RTK"
                        SignalState.RSSI_ONLY -> "RSSI Est."
                        SignalState.LOST_CONTACT -> "No Signal"
                    },
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = signalColor
                )
            }
        }

        // Flight state pill
        val fsColor = flightStateColor(state.telemetry?.flightState ?: 0)
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(50))
                .background(fsColor.copy(alpha = 0.15f))
                .padding(horizontal = 12.dp, vertical = 6.dp)
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                Icon(
                    imageVector = Icons.Rounded.FlightTakeoff,
                    contentDescription = null,
                    tint = fsColor,
                    modifier = Modifier.size(14.dp)
                )
                Text(
                    text = flightStateName(state.telemetry?.flightState ?: 0),
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = fsColor
                )
            }
        }

        Spacer(modifier = Modifier.weight(1f))

        // Last contact indicator
        if (state.signalState == SignalState.LOST_CONTACT) {
            Text(
                text = if (state.secondsSinceLastContact > 9000) "No contact"
                else "${state.secondsSinceLastContact}s ago",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error
            )
        }
    }
}

// ── Telemetry metrics grid ────────────────────────────────────────────────────

@Composable
private fun TelemetryMetrics(state: TrackingUiState) {
    val t = state.telemetry

    // Row 1: Altitudes + Velocity
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        MetricTile(Modifier.weight(1f), "Baro Alt", "%.1f m".format(t?.altitudeM ?: 0f))
        MetricTile(Modifier.weight(1f), "GPS Alt", "%.1f m".format(t?.gpsAltM ?: 0f))
        MetricTile(Modifier.weight(1f), "Velocity", "%+.1f m/s".format(t?.velocityMps ?: 0f))
    }

    // Row 2: RSSI / SNR / GPS SNR
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        MetricTile(Modifier.weight(1f), "RSSI", "${t?.rssi ?: 0} dBm")
        MetricTile(Modifier.weight(1f), "SNR", "%.1f dB".format(t?.snr ?: 0f))
        MetricTile(
            Modifier.weight(1f),
            "GPS SNR",
            if ((t?.gpsSNR ?: 0) > 0) "${t!!.gpsSNR} dB-Hz" else "—"
        )
    }

    // Row 3: RTK + Pyro
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        MetricTile(Modifier.weight(1f), "RTK Fix", t?.rtkFix ?: "—")

        // Pyro tile — color-coded
        val pyroColor = if (t?.pyroContinuity == true) MaterialTheme.colorScheme.primary
        else MaterialTheme.colorScheme.error
        MetricTileColored(
            modifier = Modifier.weight(1f),
            label = "Pyro",
            value = if (t?.pyroContinuity == true) "CONT" else "DISC",
            valueColor = pyroColor
        )

        MetricTile(Modifier.weight(1f), "Accel Z", "%.2f g".format(t?.accelZGs ?: 0f))
    }

    // GPS coordinates
    if (t != null && t.gpsLat != 0.0) {
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            MetricTile(
                Modifier.weight(1f),
                "Latitude",
                "%.5f°".format(t.gpsLat)
            )
            MetricTile(
                Modifier.weight(1f),
                "Longitude",
                "%.5f°".format(t.gpsLon)
            )
        }
    }
}

@Composable
private fun MetricTile(modifier: Modifier, label: String, value: String) {
    MetricTileColored(modifier, label, value, MaterialTheme.colorScheme.onSurface)
}

@Composable
private fun MetricTileColored(modifier: Modifier, label: String, value: String, valueColor: Color) {
    Box(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .padding(horizontal = 10.dp, vertical = 10.dp)
    ) {
        Column {
            Text(
                text = label,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = value,
                style = DataTextStyleSmall,
                color = valueColor,
                maxLines = 1
            )
        }
    }
}

// ── Bottom: Antenna pointing section ─────────────────────────────────────────

@Composable
private fun AntennaPointingSection(modifier: Modifier, uiState: TrackingUiState) {
    val animAz by animateFloatAsState(
        targetValue = uiState.deviationAzimuth.toFloat(),
        animationSpec = spring(dampingRatio = 0.6f, stiffness = 150f),
        label = "az"
    )
    val animEl by animateFloatAsState(
        targetValue = uiState.deviationElevation.toFloat(),
        animationSpec = spring(dampingRatio = 0.6f, stiffness = 150f),
        label = "el"
    )

    val inRssiScanMode = uiState.signalState == SignalState.RSSI_ONLY
    val totalDev = sqrt(animAz * animAz + animEl * animEl)
    val dotColor by animateColorAsState(
        targetValue = when {
            !uiState.distanceIsGps && !uiState.rssiHasEstimate -> MaterialTheme.colorScheme.outline
            totalDev < 3f -> DotGood
            totalDev < 12f -> DotOk
            else -> DotBad
        },
        label = "dotColor"
    )

    val distanceStr = when {
        uiState.distanceM < 1000 -> "%.0f m".format(uiState.distanceM)
        else -> "%.2f km".format(uiState.distanceM / 1000)
    }

    Column(
        modifier = modifier
            .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp))
            .background(
                Brush.verticalGradient(
                    colors = listOf(RadarBackground.copy(alpha = 0.95f), RadarBackground)
                )
            )
            .padding(top = 16.dp, start = 16.dp, end = 16.dp, bottom = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // ── Title row ─────────────────────────────────────────────────────────
        val titleText = when {
            uiState.distanceIsGps -> "Point Antenna"
            uiState.rssiHasEstimate -> "RSSI Estimate"
            inRssiScanMode -> "RSSI Scan — Slowly Pan 360°"
            else -> "Awaiting Signal"
        }
        Text(
            text = titleText,
            style = MaterialTheme.typography.labelMedium,
            color = Color.White.copy(alpha = 0.5f),
            letterSpacing = 1.5.sp
        )

        Spacer(modifier = Modifier.height(2.dp))

        // ── Distance / scan status row ────────────────────────────────────────
        when {
            uiState.distanceIsGps || uiState.rssiHasEstimate -> {
                Row(
                    verticalAlignment = Alignment.Bottom,
                    horizontalArrangement = Arrangement.Center
                ) {
                    Text(
                        text = distanceStr,
                        style = DataTextStyleLarge,
                        color = if (uiState.distanceIsGps) Color(0xFF5DD5F0)
                        else Color(0xFFFFB300)
                    )
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(
                        text = if (uiState.distanceIsGps) "GPS" else "~RSSI",
                        style = MaterialTheme.typography.labelSmall,
                        color = Color.White.copy(alpha = 0.4f),
                        modifier = Modifier.padding(bottom = 4.dp)
                    )
                }
            }

            inRssiScanMode -> {
                // Show coverage progress bar
                val coverage = uiState.rssiScanCoverage
                val pct = (coverage * 100).toInt()
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    Text(
                        text = "Coverage: $pct%",
                        style = DataTextStyleSmall,
                        color = Color.White.copy(alpha = 0.7f)
                    )
                    if (uiState.rssiMaxRssi > -120) {
                        Text(
                            text = "Peak: ${uiState.rssiMaxRssi} dBm",
                            style = DataTextStyleSmall,
                            color = Color(0xFF4DD9E7).copy(alpha = 0.8f)
                        )
                    }
                }
            }

            else -> {
                Text(
                    text = distanceStr,
                    style = DataTextStyleLarge,
                    color = Color.White.copy(alpha = 0.3f)
                )
            }
        }

        Spacer(modifier = Modifier.height(8.dp))

        // ── Reticle + deviation labels ────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            verticalAlignment = Alignment.CenterVertically
        ) {
            DeviationLabel(
                modifier = Modifier.width(52.dp),
                axis = "EL",
                value = animEl,
                positiveLabel = "UP",
                negativeLabel = "DOWN",
                valid = uiState.distanceIsGps || uiState.rssiHasEstimate
            )

            val textMeasurer = rememberTextMeasurer()
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxSize()
                    .drawBehind {
                        drawReticle(
                            textMeasurer = textMeasurer,
                            azDev = animAz,
                            elDev = animEl,
                            dotColor = dotColor,
                            hasGps = uiState.distanceIsGps,
                            rssiHasEstimate = uiState.rssiHasEstimate,
                            rssiAzimuthBins = uiState.rssiAzimuthBins,
                            inScanMode = inRssiScanMode
                        )
                    }
            )

            DeviationLabel(
                modifier = Modifier.width(52.dp),
                axis = "AZ",
                value = animAz,
                positiveLabel = "RIGHT",
                negativeLabel = "LEFT",
                valid = uiState.distanceIsGps || uiState.rssiHasEstimate
            )
        }
    }
}

@Composable
private fun DeviationLabel(
    modifier: Modifier,
    axis: String,
    value: Float,
    positiveLabel: String,
    negativeLabel: String,
    valid: Boolean = true
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(
            text = axis,
            style = MaterialTheme.typography.labelSmall,
            color = Color.White.copy(alpha = 0.4f),
            letterSpacing = 1.sp
        )
        Spacer(modifier = Modifier.height(4.dp))
        Text(
            text = if (valid) "%+.1f°".format(value) else "—",
            style = TextStyle(
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
                fontSize = 16.sp,
                color = if (valid) Color.White else Color.White.copy(alpha = 0.3f)
            )
        )
        if (valid) {
            Text(
                text = if (value > 0.5f) positiveLabel else if (value < -0.5f) negativeLabel else "CENTER",
                style = MaterialTheme.typography.labelSmall,
                color = Color(0xFF5DD5F0).copy(alpha = 0.8f),
                fontSize = 9.sp
            )
        }
    }
}

// ── Canvas: the concentric circle aiming reticle ─────────────────────────────

private fun DrawScope.drawReticle(
    textMeasurer: TextMeasurer,
    azDev: Float,
    elDev: Float,
    dotColor: Color,
    hasGps: Boolean,
    rssiHasEstimate: Boolean = false,
    rssiAzimuthBins: List<Boolean> = emptyList(),
    inScanMode: Boolean = false
) {
    val cx = size.width / 2f
    val cy = size.height / 2f
    val maxRadius = minOf(size.width, size.height) * 0.44f

    // ── Scan coverage ring (outermost, drawn first so it's below grid) ────────
    if (inScanMode && rssiAzimuthBins.isNotEmpty()) {
        val binRadius = maxRadius * 1.13f
        val binSize = Size(binRadius * 2, binRadius * 2)
        val binTopLeft = Offset(cx - binRadius, cy - binRadius)
        rssiAzimuthBins.forEachIndexed { i, sampled ->
            // Convert compass bin (0=north, CW) to canvas angle (0=right, CW)
            val canvasStart = (i * 10f - 90f)
            drawArc(
                color = if (sampled) DotGood.copy(alpha = 0.75f)
                else Color(0xFF1A2A30).copy(alpha = 0.9f),
                startAngle = canvasStart,
                sweepAngle = 9f, // 10° bin - 1° gap
                useCenter = false,
                topLeft = binTopLeft,
                size = binSize,
                style = Stroke(width = 4.dp.toPx())
            )
        }
    }

    // ── Degree rings ──────────────────────────────────────────────────────────
    val ringDegs = listOf(5f, 10f, 15f, 20f, 30f)
    val maxDeg = 30f

    ringDegs.forEach { deg ->
        val r = (deg / maxDeg) * maxRadius
        val alpha = 0.08f + 0.14f * (1f - deg / maxDeg)
        drawCircle(
            color = RadarGrid.copy(alpha = alpha),
            radius = r,
            center = Offset(cx, cy),
            style = Stroke(width = 1.dp.toPx())
        )
    }

    // ── Crosshair ─────────────────────────────────────────────────────────────
    val crossAlpha = 0.2f
    drawLine(RadarGrid.copy(alpha = crossAlpha), Offset(cx - maxRadius, cy), Offset(cx + maxRadius, cy), 1.dp.toPx())
    drawLine(RadarGrid.copy(alpha = crossAlpha), Offset(cx, cy - maxRadius), Offset(cx, cy + maxRadius), 1.dp.toPx())

    // ── Degree labels on each ring ────────────────────────────────────────────
    val labelStyle = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontSize = 9.sp,
        color = RadarGrid.copy(alpha = 0.6f)
    )
    ringDegs.forEach { deg ->
        val r = (deg / maxDeg) * maxRadius
        val measured = textMeasurer.measure("${deg.toInt()}°", labelStyle)
        val labelX = cx + r - measured.size.width - 4.dp.toPx()
        val labelY = cy - measured.size.height / 2f
        drawText(measured, topLeft = Offset(labelX, labelY))
    }

    // ── Compass labels ────────────────────────────────────────────────────────
    val compassLabels = listOf("N" to 0f, "E" to 90f, "S" to 180f, "W" to 270f)
    compassLabels.forEach { (label, angleDeg) ->
        val rad = Math.toRadians((angleDeg - 90).toDouble())
        val lx = cx + (maxRadius + 8.dp.toPx()) * cos(rad).toFloat()
        val ly = cy + (maxRadius + 8.dp.toPx()) * sin(rad).toFloat()
        val m = textMeasurer.measure(
            label, TextStyle(
                fontSize = 9.sp,
                fontWeight = FontWeight.Bold,
                color = RadarGrid.copy(alpha = 0.35f),
                fontFamily = FontFamily.Monospace
            )
        )
        drawText(m, topLeft = Offset(lx - m.size.width / 2f, ly - m.size.height / 2f))
    }

    // ── Center status label when no target ────────────────────────────────────
    val noTargetLabel = when {
        inScanMode && !rssiHasEstimate -> "PAN TO SCAN"
        !hasGps && !rssiHasEstimate -> "NO TARGET"
        else -> null
    }
    noTargetLabel?.let { label ->
        val m = textMeasurer.measure(
            label, TextStyle(
                fontSize = 10.sp,
                color = RadarGrid.copy(alpha = 0.4f),
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold
            )
        )
        drawText(m, topLeft = Offset(cx - m.size.width / 2f, cy + 14.dp.toPx()))
    }

    // ── Deviation dot (only when we have a target) ────────────────────────────
    if (!hasGps && !rssiHasEstimate) return

    val dotXFrac = (azDev / maxDeg).coerceIn(-1f, 1f)
    val dotYFrac = (-elDev / maxDeg).coerceIn(-1f, 1f)
    val dotX = cx + dotXFrac * maxRadius
    val dotY = cy + dotYFrac * maxRadius

    drawLine(
        color = dotColor.copy(alpha = 0.6f),
        start = Offset(cx, cy),
        end = Offset(dotX, dotY),
        strokeWidth = 1.5.dp.toPx(),
        pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 5f))
    )

    drawCircle(dotColor.copy(alpha = 0.12f), 22.dp.toPx(), Offset(dotX, dotY))
    drawCircle(dotColor.copy(alpha = 0.25f), 12.dp.toPx(), Offset(dotX, dotY))
    drawCircle(dotColor, 5.dp.toPx(), Offset(dotX, dotY))

    // ── Center bullseye ───────────────────────────────────────────────────────
    drawCircle(Color.White.copy(alpha = 0.9f), 5.dp.toPx(), Offset(cx, cy), style = Stroke(1.5.dp.toPx()))
    drawCircle(Color.White.copy(alpha = 0.5f), 1.5.dp.toPx(), Offset(cx, cy))
}
