package com.example.pinpointer.ui.tracking

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.example.pinpointer.ui.theme.DataTextStyle
import com.example.pinpointer.ui.theme.DataTextStyleLarge
import com.example.pinpointer.ui.theme.DataTextStyleSmall
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
import kotlinx.coroutines.delay
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

// ── Helpers ───────────────────────────────────────────────────────────────────

private fun flightStateName(s: Int) = when (s) {
    0 -> "Standby"; 1 -> "Motor Burn"; 2 -> "Coast"
    3 -> "Freefall"; 4 -> "Landed"; else -> "Unknown"
}

private fun flightStateColor(s: Int) = when (s) {
    0 -> FlightStateStandby; 1 -> FlightStateMotorBurn; 2 -> FlightStateCoast
    3 -> FlightStateFreefall; 4 -> FlightStateLanded; else -> FlightStateStandby
}

/** Human-readable GPS fix label from the raw rtkFix string. */
private fun rtkFixLabel(fix: String?) = when (fix) {
    "RTK-Fixed" -> "RTK Fixed"
    "RTK-Float" -> "RTK Float"
    "DGPS", "DgpsFix" -> "DGPS"
    "GPS", "GpsFix" -> "GPS"
    "PPS", "PpsFix" -> "PPS"
    "DeadReckoning" -> "Dead Reck."
    else -> "No Fix"
}

enum class ValueTrend { RISING, FALLING, STABLE }

/** Returns the rolling trend of a float value, updating reference every ~2 s. */
@Composable
private fun rememberTrend(value: Float, threshold: Float = 0.05f): ValueTrend {
    var ref by remember { mutableFloatStateOf(value) }
    var trend by remember { mutableStateOf(ValueTrend.STABLE) }
    LaunchedEffect(value) {
        trend = when {
            value > ref + threshold -> ValueTrend.RISING
            value < ref - threshold -> ValueTrend.FALLING
            else -> ValueTrend.STABLE
        }
        delay(2_000L)
        ref = value
    }
    return trend
}

private val trendColorRising = Color(0xFF4CAF50)
private val trendColorFalling = Color(0xFFF44336)
private val trendColorStable = Color(0xFF78909C)

private fun ValueTrend.color() = when (this) {
    ValueTrend.RISING -> trendColorRising
    ValueTrend.FALLING -> trendColorFalling
    ValueTrend.STABLE -> trendColorStable
}

private fun ValueTrend.arrow() = when (this) {
    ValueTrend.RISING -> "↑"
    ValueTrend.FALLING -> "↓"
    ValueTrend.STABLE -> "→"
}

// ── Root composable ───────────────────────────────────────────────────────────

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TrackingScreen(viewModel: TrackingViewModel) {
    val state by viewModel.uiState.collectAsState()
    val t = state.telemetry

    // GPS altitude relative/absolute toggle
    var gpsAltRelative by remember { mutableStateOf(true) }
    var firstGpsAlt by remember { mutableStateOf<Float?>(null) }
    LaunchedEffect(t?.gpsAltM) {
        val alt = t?.gpsAltM ?: return@LaunchedEffect
        if (alt != 0f && firstGpsAlt == null) firstGpsAlt = alt
    }

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        Column(modifier = Modifier.fillMaxSize()) {
            // ── TOP: scrollable telemetry ──────────────────────────────────────
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f, fill = false)
                    .heightIn(max = 420.dp)        // cap so bottom reticle always shows
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 14.dp, vertical = 10.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                // ── Pill row ───────────────────────────────────────────────────
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp)
                ) {
                    SignalPill(state)
                    FlightStatePill(t?.flightState ?: 0)
                    PyroPill(t?.pyroContinuity ?: false, t?.pyroDeployed ?: false)
                    if (state.isForceRssiMode) RssiModePill()
                }

                // ── Tier 1: Big altitude cards ─────────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    BigMetricCard(
                        modifier = Modifier.weight(1f),
                        label = "Baro Alt",
                        value = t?.altitudeM ?: 0f,
                        formatValue = { "%.1f m".format(it) },
                        trendThreshold = 0.3f
                    )
                    BigMetricCard(
                        modifier = Modifier.weight(1f),
                        label = if (gpsAltRelative) "GPS Alt (AGL)" else "GPS Alt (MSL)",
                        value = if (gpsAltRelative && firstGpsAlt != null)
                            (t?.gpsAltM ?: 0f) - (firstGpsAlt ?: 0f)
                        else t?.gpsAltM ?: 0f,
                        formatValue = { "%.1f m".format(it) },
                        trendThreshold = 0.3f,
                        clickLabel = if (gpsAltRelative) "Switch to MSL" else "Switch to AGL",
                        onClick = { gpsAltRelative = !gpsAltRelative },
                        badge = if (gpsAltRelative) "AGL ⇄" else "MSL ⇄"
                    )
                }

                // ── Tier 2: RSSI + RTK ─────────────────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    MediumMetricCard(
                        modifier = Modifier.weight(1f),
                        label = "RSSI",
                        value = (t?.rssi ?: 0).toFloat(),
                        formatValue = { "${it.toInt()} dBm" },
                        trendThreshold = 1f
                    )
                    RtkFixCard(
                        modifier = Modifier.weight(1f),
                        rtkFix = t?.rtkFix
                    )
                }

                // ── Tier 3: Velocity / Accel / SNR ────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    SmallMetricCard(
                        Modifier.weight(1f), "Velocity",
                        (t?.velocityMps ?: 0f),
                        { "%+.1f m/s".format(it) }, 0.1f
                    )
                    SmallMetricCard(
                        Modifier.weight(1f), "Accel Z",
                        (t?.accelZGs ?: 0f),
                        { "%.2f g".format(it) }, 0.05f
                    )
                    SmallMetricCard(
                        Modifier.weight(1f), "SNR",
                        (t?.snr ?: 0f),
                        { "%.1f dB".format(it) }, 0.5f
                    )
                }

                // ── GPS coordinates (compact) ──────────────────────────────────
                if (t != null && t.gpsLat != 0.0) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(MaterialTheme.shapes.medium)
                            .background(MaterialTheme.colorScheme.surfaceContainer)
                            .padding(horizontal = 12.dp, vertical = 6.dp)
                    ) {
                        Text(
                            text = "GPS  %.5f°  %.5f°".format(t.gpsLat, t.gpsLon),
                            style = DataTextStyleSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                }
            }

            // ── BOTTOM: antenna pointing (takes all remaining height) ──────────
            AntennaPointingSection(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                uiState = state
            )
        }
    }
}

// ── Status pills ──────────────────────────────────────────────────────────────

@Composable
private fun StatusPill(
    label: String,
    color: Color,
    icon: @Composable (() -> Unit)? = null
) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(50))
            .background(color.copy(alpha = 0.15f))
            .padding(horizontal = 10.dp, vertical = 5.dp)
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp)
        ) {
            icon?.invoke()
            Text(
                text = label,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.SemiBold,
                color = color,
                maxLines = 1
            )
        }
    }
}

@Composable
private fun SignalPill(state: TrackingUiState) {
    val color by animateColorAsState(
        when (state.signalState) {
            SignalState.GPS_FIX -> MaterialTheme.colorScheme.primary
            SignalState.RSSI_ONLY -> MaterialTheme.colorScheme.tertiary
            SignalState.LOST_CONTACT -> MaterialTheme.colorScheme.error
        }, label = "sigCol"
    )
    val label = when (state.signalState) {
        SignalState.GPS_FIX -> rtkFixLabel(state.telemetry?.rtkFix)
        SignalState.RSSI_ONLY -> if (state.rssiHasEstimate) "RSSI Est." else "RSSI Scan"
        SignalState.LOST_CONTACT -> if (state.secondsSinceLastContact > 9000) "No Contact"
        else "${state.secondsSinceLastContact}s ago"
    }
    StatusPill(label, color) {
        Icon(
            imageVector = when (state.signalState) {
                SignalState.GPS_FIX -> Icons.Rounded.GpsFixed
                SignalState.RSSI_ONLY -> Icons.Rounded.GpsOff
                SignalState.LOST_CONTACT -> Icons.Rounded.SignalCellularOff
            },
            contentDescription = null,
            tint = color,
            modifier = Modifier.size(13.dp)
        )
    }
}

@Composable
private fun FlightStatePill(state: Int) {
    val color = flightStateColor(state)
    StatusPill(flightStateName(state), color) {
        Icon(Icons.Rounded.FlightTakeoff, null, tint = color, modifier = Modifier.size(13.dp))
    }
}

@Composable
private fun PyroPill(continuity: Boolean, deployed: Boolean) {
    val (label, color) = when {
        deployed -> "Pyro Fired" to MaterialTheme.colorScheme.error
        continuity -> "Pyro OK" to MaterialTheme.colorScheme.primary
        else -> "Pyro DISC" to MaterialTheme.colorScheme.error
    }
    StatusPill(label, color)
}

@Composable
private fun RssiModePill() {
    StatusPill("RSSI Mode", MaterialTheme.colorScheme.tertiary) {
        Icon(
            Icons.Rounded.Tune, null,
            tint = MaterialTheme.colorScheme.tertiary, modifier = Modifier.size(13.dp)
        )
    }
}

// ── Metric cards ──────────────────────────────────────────────────────────────

@Composable
private fun BigMetricCard(
    modifier: Modifier,
    label: String,
    value: Float,
    formatValue: (Float) -> String,
    trendThreshold: Float,
    badge: String? = null,
    clickLabel: String? = null,
    onClick: (() -> Unit)? = null
) {
    val trend = rememberTrend(value, trendThreshold)
    val animVal by animateFloatAsState(value, label = "bigMetric_$label")

    Box(
        modifier = modifier
            .clip(MaterialTheme.shapes.large)
            .background(MaterialTheme.colorScheme.surfaceContainerHigh)
            .then(if (onClick != null) Modifier.clickable(onClickLabel = clickLabel) { onClick() } else Modifier)
            .padding(12.dp)
    ) {
        Column {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = label,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                badge?.let {
                    Text(
                        it, style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold
                    )
                }
            }
            Spacer(Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = trend.arrow(),
                    style = TextStyle(
                        fontFamily = FontFamily.Default,
                        fontWeight = FontWeight.Bold,
                        fontSize = 20.sp,
                        color = trend.color()
                    )
                )
                Spacer(Modifier.width(4.dp))
                Text(
                    text = formatValue(animVal),
                    style = DataTextStyleLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }
    }
}

@Composable
private fun MediumMetricCard(
    modifier: Modifier,
    label: String,
    value: Float,
    formatValue: (Float) -> String,
    trendThreshold: Float
) {
    val trend = rememberTrend(value, trendThreshold)
    val animVal by animateFloatAsState(value, label = "medMetric_$label")

    Box(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .padding(horizontal = 12.dp, vertical = 10.dp)
    ) {
        Column {
            Text(
                label, style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = trend.arrow(),
                    style = TextStyle(
                        fontFamily = FontFamily.Default,
                        fontWeight = FontWeight.Bold,
                        fontSize = 16.sp,
                        color = trend.color()
                    )
                )
                Spacer(Modifier.width(3.dp))
                Text(
                    text = formatValue(animVal),
                    style = DataTextStyle,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1
                )
            }
        }
    }
}

@Composable
private fun RtkFixCard(modifier: Modifier, rtkFix: String?) {
    val label = rtkFixLabel(rtkFix)
    val color = when (rtkFix) {
        "RTK-Fixed" -> MaterialTheme.colorScheme.primary
        "RTK-Float" -> MaterialTheme.colorScheme.tertiary
        "DGPS", "DgpsFix", "GPS", "GpsFix", "PPS", "PpsFix"
            -> MaterialTheme.colorScheme.secondary

        else -> MaterialTheme.colorScheme.error
    }
    Box(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .padding(horizontal = 12.dp, vertical = 10.dp)
    ) {
        Column {
            Text(
                "RTK Fix", style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(Modifier.height(4.dp))
            Text(
                text = label,
                style = DataTextStyle,
                color = color,
                fontWeight = FontWeight.Bold,
                maxLines = 1
            )
        }
    }
}

@Composable
private fun SmallMetricCard(
    modifier: Modifier,
    label: String,
    value: Float,
    formatValue: (Float) -> String,
    trendThreshold: Float
) {
    val trend = rememberTrend(value, trendThreshold)
    val animVal by animateFloatAsState(value, label = "smMetric_$label")

    Box(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .padding(horizontal = 10.dp, vertical = 8.dp)
    ) {
        Column {
            Text(
                label, style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(Modifier.height(2.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = trend.arrow(),
                    style = TextStyle(
                        fontFamily = FontFamily.Default,
                        fontWeight = FontWeight.Bold,
                        fontSize = 12.sp,
                        color = trend.color()
                    )
                )
                Spacer(Modifier.width(2.dp))
                Text(
                    text = formatValue(animVal),
                    style = DataTextStyleSmall,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }
    }
}

// ── Antenna pointing section ──────────────────────────────────────────────────

@Composable
private fun AntennaPointingSection(modifier: Modifier, uiState: TrackingUiState) {
    val animAz by animateFloatAsState(
        uiState.deviationAzimuth.toFloat(),
        animationSpec = spring(dampingRatio = 0.6f, stiffness = 150f), label = "az"
    )
    val animEl by animateFloatAsState(
        uiState.deviationElevation.toFloat(),
        animationSpec = spring(dampingRatio = 0.6f, stiffness = 150f), label = "el"
    )

    val inRssiScanMode = uiState.signalState == SignalState.RSSI_ONLY
    val totalDev = sqrt(animAz * animAz + animEl * animEl)
    val dotColor by animateColorAsState(
        when {
            !uiState.distanceIsGps && !uiState.rssiHasEstimate -> MaterialTheme.colorScheme.outline
            totalDev < 3f -> DotGood
            totalDev < 12f -> DotOk
            else -> DotBad
        }, label = "dotC"
    )

    val distStr = if (uiState.distanceM < 1000) "%.0f m".format(uiState.distanceM)
    else "%.2f km".format(uiState.distanceM / 1000)

    // Container follows the MD3 surface token so it looks correct in both themes.
    // Only the circular radar canvas stays always-dark (it IS a radar display).
    val containerColor = MaterialTheme.colorScheme.surfaceContainerHighest
    val onContainer = MaterialTheme.colorScheme.onSurface

    Column(
        modifier = modifier
            .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp))
            .background(containerColor)
            .padding(top = 12.dp, start = 14.dp, end = 14.dp, bottom = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // ── Title ─────────────────────────────────────────────────────────────
        Text(
            text = when {
                uiState.distanceIsGps -> "Point Antenna"
                uiState.rssiHasEstimate -> "RSSI Estimate"
                inRssiScanMode -> "RSSI Scan — Slowly Pan 360°"
                else -> "Awaiting Signal"
            },
            style = MaterialTheme.typography.labelMedium,
            color = onContainer.copy(alpha = 0.45f),
            letterSpacing = 1.5.sp
        )
        Spacer(Modifier.height(2.dp))

        // ── Distance / scan-progress row ──────────────────────────────────────
        when {
            uiState.distanceIsGps || uiState.rssiHasEstimate -> {
                Row(verticalAlignment = Alignment.Bottom) {
                    Text(
                        distStr,
                        style = TextStyle(
                            fontFamily = FontFamily.Monospace,
                            fontWeight = FontWeight.Bold,
                            fontSize = 38.sp
                        ),
                        color = if (uiState.distanceIsGps) MaterialTheme.colorScheme.primary
                        else Color(0xFFFFB300)
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        if (uiState.distanceIsGps) "GPS" else "~RSSI",
                        style = MaterialTheme.typography.labelSmall,
                        color = onContainer.copy(alpha = 0.4f),
                        modifier = Modifier.padding(bottom = 6.dp)
                    )
                }
            }

            inRssiScanMode -> {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    Text(
                        "Coverage: ${(uiState.rssiScanCoverage * 100).toInt()}%",
                        style = DataTextStyleSmall,
                        color = onContainer.copy(alpha = 0.7f)
                    )
                    if (uiState.rssiMaxRssi > -120) {
                        Text(
                            "Peak: ${uiState.rssiMaxRssi} dBm",
                            style = DataTextStyleSmall,
                            color = MaterialTheme.colorScheme.primary
                        )
                    }
                }
            }

            else -> Text(
                distStr,
                style = TextStyle(
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.Bold,
                    fontSize = 38.sp
                ),
                color = onContainer.copy(alpha = 0.2f)
            )
        }

        Spacer(Modifier.height(6.dp))

        // ── Dark radar canvas (fills remaining height, full width) ─────────────
        val textMeasurer = rememberTextMeasurer()
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .clip(RoundedCornerShape(20.dp))
                .background(Color(0xFF060D14))   // always dark — it is a radar display
                .drawBehind {
                    drawReticle(
                        textMeasurer, animAz, animEl, dotColor,
                        uiState.distanceIsGps, uiState.rssiHasEstimate,
                        uiState.rssiAzimuthBins, inRssiScanMode
                    )
                }
        )

        Spacer(Modifier.height(8.dp))

        // ── Deviation labels (below the canvas) ───────────────────────────────
        val valid = uiState.distanceIsGps || uiState.rssiHasEstimate
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.CenterVertically
        ) {
            DeviationBlock("ELEVATION", animEl, "UP", "DOWN", valid, onContainer)
            // Vertical divider
            Box(
                Modifier
                    .width(1.dp)
                    .height(52.dp)
                    .background(onContainer.copy(alpha = 0.15f))
            )
            DeviationBlock("AZIMUTH", animAz, "RIGHT", "LEFT", valid, onContainer)
        }
    }
}

@Composable
private fun DeviationBlock(
    axis: String,
    value: Float,
    positiveLabel: String,
    negativeLabel: String,
    valid: Boolean,
    onSurface: Color
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(
            axis,
            style = MaterialTheme.typography.labelSmall,
            color = onSurface.copy(alpha = 0.4f),
            letterSpacing = 1.5.sp
        )
        Spacer(Modifier.height(2.dp))
        Text(
            text = if (valid) "%+.1f°".format(value) else "—",
            style = TextStyle(
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
                fontSize = 26.sp,
                color = if (valid) onSurface else onSurface.copy(alpha = 0.25f)
            )
        )
        if (valid) {
            Text(
                text = when {
                    value > 0.5f -> positiveLabel
                    value < -0.5f -> negativeLabel
                    else -> "CENTER"
                },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.8f),
                fontSize = 9.sp
            )
        }
    }
}

// ── Canvas reticle ────────────────────────────────────────────────────────────

private fun DrawScope.drawReticle(
    textMeasurer: TextMeasurer,
    azDev: Float, elDev: Float,
    dotColor: Color,
    hasGps: Boolean,
    rssiHasEstimate: Boolean = false,
    rssiAzimuthBins: List<Boolean> = emptyList(),
    inScanMode: Boolean = false
) {
    val cx = size.width / 2f
    val cy = size.height / 2f
    val maxRadius = minOf(size.width, size.height) * 0.44f

    // Scan coverage ring
    if (inScanMode && rssiAzimuthBins.isNotEmpty()) {
        val binR = maxRadius * 1.13f
        val binSz = Size(binR * 2, binR * 2)
        val binTL = Offset(cx - binR, cy - binR)
        rssiAzimuthBins.forEachIndexed { i, sampled ->
            drawArc(
                color = if (sampled) DotGood.copy(alpha = 0.75f)
                else Color(0xFF1A2A30).copy(alpha = 0.9f),
                startAngle = i * 10f - 90f,
                sweepAngle = 9f,
                useCenter = false,
                topLeft = binTL, size = binSz,
                style = Stroke(width = 4.dp.toPx())
            )
        }
    }

    // Degree rings
    val ringDegs = listOf(5f, 10f, 15f, 20f, 30f)
    val maxDeg = 30f
    ringDegs.forEach { deg ->
        val r = (deg / maxDeg) * maxRadius
        drawCircle(
            color = RadarGrid.copy(alpha = 0.08f + 0.14f * (1f - deg / maxDeg)),
            radius = r, center = Offset(cx, cy),
            style = Stroke(1.dp.toPx())
        )
    }

    // Crosshair
    drawLine(RadarGrid.copy(0.2f), Offset(cx - maxRadius, cy), Offset(cx + maxRadius, cy), 1.dp.toPx())
    drawLine(RadarGrid.copy(0.2f), Offset(cx, cy - maxRadius), Offset(cx, cy + maxRadius), 1.dp.toPx())

    // Degree labels
    val lblStyle = TextStyle(
        fontFamily = FontFamily.Monospace, fontSize = 9.sp,
        color = RadarGrid.copy(alpha = 0.6f)
    )
    ringDegs.forEach { deg ->
        val r = (deg / maxDeg) * maxRadius
        val m = textMeasurer.measure("${deg.toInt()}°", lblStyle)
        drawText(m, topLeft = Offset(cx + r - m.size.width - 4.dp.toPx(), cy - m.size.height / 2f))
    }

    // Compass labels
    listOf("N" to 0f, "E" to 90f, "S" to 180f, "W" to 270f).forEach { (lbl, ang) ->
        val rad = Math.toRadians((ang - 90).toDouble())
        val lx = cx + (maxRadius + 8.dp.toPx()) * cos(rad).toFloat()
        val ly = cy + (maxRadius + 8.dp.toPx()) * sin(rad).toFloat()
        val m = textMeasurer.measure(
            lbl, TextStyle(
                fontSize = 9.sp, fontWeight = FontWeight.Bold,
                color = RadarGrid.copy(alpha = 0.35f), fontFamily = FontFamily.Monospace
            )
        )
        drawText(m, topLeft = Offset(lx - m.size.width / 2f, ly - m.size.height / 2f))
    }

    // Center label when no target
    val noTargetLabel = when {
        inScanMode && !rssiHasEstimate -> "PAN TO SCAN"
        !hasGps && !rssiHasEstimate -> "NO TARGET"
        else -> null
    }
    noTargetLabel?.let { lbl ->
        val m = textMeasurer.measure(
            lbl, TextStyle(
                fontSize = 10.sp,
                color = RadarGrid.copy(alpha = 0.4f), fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold
            )
        )
        drawText(m, topLeft = Offset(cx - m.size.width / 2f, cy + 14.dp.toPx()))
    }

    if (!hasGps && !rssiHasEstimate) return

    // Deviation dot
    val dotX = cx + (azDev / maxDeg).coerceIn(-1f, 1f) * maxRadius
    val dotY = cy + (-elDev / maxDeg).coerceIn(-1f, 1f) * maxRadius

    drawLine(
        color = dotColor.copy(alpha = 0.6f),
        start = Offset(cx, cy), end = Offset(dotX, dotY),
        strokeWidth = 1.5.dp.toPx(),
        pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 5f))
    )
    drawCircle(dotColor.copy(alpha = 0.12f), 22.dp.toPx(), Offset(dotX, dotY))
    drawCircle(dotColor.copy(alpha = 0.25f), 12.dp.toPx(), Offset(dotX, dotY))
    drawCircle(dotColor, 5.dp.toPx(), Offset(dotX, dotY))

    // Bullseye
    drawCircle(Color.White.copy(alpha = 0.9f), 5.dp.toPx(), Offset(cx, cy), style = Stroke(1.5.dp.toPx()))
    drawCircle(Color.White.copy(alpha = 0.5f), 1.5.dp.toPx(), Offset(cx, cy))
}
