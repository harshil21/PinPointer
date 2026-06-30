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
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
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
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Satellite
import androidx.compose.material.icons.rounded.SignalCellularOff
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
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

@OptIn(ExperimentalLayoutApi::class, ExperimentalMaterial3Api::class)
@Composable
fun TrackingScreen(viewModel: TrackingViewModel) {
    val state by viewModel.uiState.collectAsState()
    val t = state.telemetry
    val snrSummary = remember(t, state.status?.rocketDebugSnr) {
        rocketSnrSummary(t, state.status?.rocketDebugSnr)
    }

    // GPS altitude relative/absolute toggle
    var gpsAltRelative by remember { mutableStateOf(true) }
    var firstGpsAlt by remember { mutableStateOf<Float?>(null) }
    LaunchedEffect(t?.gpsAltM) {
        val alt = t?.gpsAltM ?: return@LaunchedEffect
        if (alt != 0f && firstGpsAlt == null) firstGpsAlt = alt
    }

    // Rocket SNR detail sheet
    var snrSheetOpen by remember { mutableStateOf(false) }
    val snrSheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val sheetScope = rememberCoroutineScope()

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
                        formatValue = { "%.2f m".format(it) },
                        trendThreshold = 0.3f
                    )
                    BigMetricCard(
                        modifier = Modifier.weight(1f),
                        label = if (gpsAltRelative) "GPS Alt (AGL)" else "GPS Alt (MSL)",
                        value = if (gpsAltRelative && firstGpsAlt != null)
                            (t?.gpsAltM ?: 0f) - (firstGpsAlt ?: 0f)
                        else t?.gpsAltM ?: 0f,
                        formatValue = { "%.2f m".format(it) },
                        trendThreshold = 0.3f,
                        clickLabel = if (gpsAltRelative) "Switch to MSL" else "Switch to AGL",
                        onClick = { gpsAltRelative = !gpsAltRelative },
                        badge = if (gpsAltRelative) "AGL ⇄" else "MSL ⇄"
                    )
                }

                // ── Tier 2: RSSI + RTK ─────────────────────────────────────────
                // ── Tier 2: RSSI + RTK + GPS SNR ─────────────────────────────────────────────────────────────────────
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
                    // Rocket GPS SNR — tap to see per-constellation breakdown.
                    RocketSnrCard(
                        modifier = Modifier.weight(1f),
                        summary = snrSummary,
                        onClick = { snrSheetOpen = true }
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
                            text = "GPS  %.8f°  %.8f°".format(t.gpsLat, t.gpsLon),
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
                uiState = state,
                onRecalibrate = { viewModel.recalibrateRssiTracking() }
            )
        }
    }

    if (snrSheetOpen) {
        ModalBottomSheet(
            onDismissRequest = { snrSheetOpen = false },
            sheetState = snrSheetState,
            contentWindowInsets = { WindowInsets.navigationBars }
        ) {
            RocketSnrDetails(
                summary = snrSummary,
                onClose = {
                    sheetScope.launch { snrSheetState.hide() }
                        .invokeOnCompletion { if (!snrSheetState.isVisible) snrSheetOpen = false }
                }
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
private fun AntennaPointingSection(
    modifier: Modifier,
    uiState: TrackingUiState,
    onRecalibrate: () -> Unit
) {
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

    val distStr = if (uiState.distanceM < 1000) "%.2f m".format(uiState.distanceM)
    else "%.3f km".format(uiState.distanceM / 1000f)

    // Container and radar canvas both follow the active theme — no hardcoded dark bg.
    val containerColor = MaterialTheme.colorScheme.background
    val onContainer = MaterialTheme.colorScheme.onSurface
    // Inside the canvas, derive radar grid colours from the primary token so
    // they contrast with the background in both light and dark mode.
    val radarGridColor = MaterialTheme.colorScheme.primary
    val radarLabelColor = MaterialTheme.colorScheme.onSurface

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
                uiState.targetSource == "GPS + RSSI" -> "Point Antenna - GPS + RSSI"
                uiState.distanceIsGps -> "Point Antenna - GPS"
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
                        uiState.targetSource,
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
                .background(MaterialTheme.colorScheme.surfaceContainer) // slight tint for the canvas area
                .drawBehind {
                    drawReticle(
                        textMeasurer, animAz, animEl, dotColor,
                        uiState.distanceIsGps, uiState.rssiHasEstimate,
                        uiState.rssiAzimuthBins, inRssiScanMode,
                        gridColor = radarGridColor,
                        labelColor = radarLabelColor
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

        Spacer(Modifier.height(8.dp))

        // ── Recalibrate (only in RSSI tracking mode) ──────────────────────────
        RecalibrateButton(
            enabled = inRssiScanMode,
            onClick = onRecalibrate
        )
    }
}

@Composable
private fun RecalibrateButton(enabled: Boolean, onClick: () -> Unit) {
    FilledTonalButton(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 44.dp)
    ) {
        Icon(
            imageVector = Icons.Rounded.Refresh,
            contentDescription = null,
            modifier = Modifier.size(18.dp)
        )
        Spacer(Modifier.width(8.dp))
        Text(
            text = if (enabled) "Recalibrate — Pan Phone Again"
            else "Recalibrate (RSSI mode only)",
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.SemiBold
        )
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
    inScanMode: Boolean = false,
    gridColor: Color = RadarGrid,
    labelColor: Color = Color.White
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
                else gridColor.copy(alpha = 0.15f),
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
            color = gridColor.copy(alpha = 0.08f + 0.18f * (1f - deg / maxDeg)),
            radius = r, center = Offset(cx, cy),
            style = Stroke(1.dp.toPx())
        )
    }

    // Crosshair
    drawLine(gridColor.copy(0.15f), Offset(cx - maxRadius, cy), Offset(cx + maxRadius, cy), 1.dp.toPx())
    drawLine(gridColor.copy(0.15f), Offset(cx, cy - maxRadius), Offset(cx, cy + maxRadius), 1.dp.toPx())

    // Degree labels
    val lblStyle = TextStyle(
        fontFamily = FontFamily.Monospace, fontSize = 9.sp,
        color = labelColor.copy(alpha = 0.5f)
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
                color = labelColor.copy(alpha = 0.35f), fontFamily = FontFamily.Monospace
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
                color = labelColor.copy(alpha = 0.4f), fontFamily = FontFamily.Monospace,
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
    drawCircle(gridColor.copy(alpha = 0.9f), 5.dp.toPx(), Offset(cx, cy), style = Stroke(1.5.dp.toPx()))
    drawCircle(gridColor.copy(alpha = 0.5f), 1.5.dp.toPx(), Offset(cx, cy))
}

// ── Rocket SNR tile + detail sheet ───────────────────────────────────────────

@Composable
private fun RocketSnrCard(
    modifier: Modifier,
    summary: RocketSnrSummary,
    onClick: () -> Unit
) {
    val animVal by animateFloatAsState(summary.displayValue.toFloat(), label = "snrCard")
    val tone = snrTone(summary.displayValue)
    Box(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .clickable(onClickLabel = "Show SNR details") { onClick() }
            .padding(horizontal = 10.dp, vertical = 8.dp)
    ) {
        Column {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    "Rkt SNR", style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Icon(
                    imageVector = Icons.Rounded.Satellite,
                    contentDescription = null,
                    tint = tone,
                    modifier = Modifier.size(12.dp)
                )
            }
            Spacer(Modifier.height(2.dp))
            Text(
                text = if (summary.hasValue) "${animVal.toInt()} dBHz" else "—",
                style = DataTextStyleSmall,
                color = if (summary.hasValue) MaterialTheme.colorScheme.onSurface
                else MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1
            )
        }
    }
}

private fun snrTone(snr: Int): Color = when {
    snr >= 35 -> Color(0xFF4CAF50)
    snr >= 25 -> Color(0xFFFFB300)
    snr > 0 -> Color(0xFFF44336)
    else -> Color(0xFF78909C)
}

@Composable
private fun RocketSnrDetails(summary: RocketSnrSummary, onClose: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 24.dp)
            .padding(bottom = 24.dp, top = 4.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                imageVector = Icons.Rounded.Satellite,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(22.dp)
            )
            Spacer(Modifier.width(10.dp))
            Text(
                text = "Rocket GPS SNR",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold
            )
        }

        val headline = when {
            summary.hasValue -> "${summary.displayValue} dB-Hz"
            else -> "No SNR data yet"
        }
        Text(
            text = headline,
            style = MaterialTheme.typography.headlineMedium,
            color = snrTone(summary.displayValue),
            fontWeight = FontWeight.Bold
        )
        Text(
            text = sourceCaption(summary.source),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )

        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)

        Text(
            text = "Per-constellation",
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.SemiBold
        )

        val per = summary.perConstellation
        if (per == null) {
            Text(
                "Enable Debug Telemetry from the Commands tab to receive per-constellation SNR from the rocket.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        } else if (!per.hasData
            && per.gps == 0 && per.glonass == 0
            && per.galileo == 0 && per.beidou == 0 && per.qzss == 0
        ) {
            Text(
                "Awaiting first debug packet from rocket…",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        } else {
            SnrDetailRow("GPS", per.gps)
            SnrDetailRow("GLONASS", per.glonass)
            SnrDetailRow("Galileo", per.galileo)
            SnrDetailRow("BeiDou", per.beidou)
            SnrDetailRow("QZSS", per.qzss)
        }

        FilledTonalButton(
            onClick = onClose,
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 8.dp)
        ) { Text("Close") }
    }
}

@Composable
private fun SnrDetailRow(name: String, snr: Int) {
    val tone = snrTone(snr)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            name,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface
        )
        Text(
            text = if (snr > 0) "$snr dB-Hz" else "—",
            style = DataTextStyleSmall,
            color = if (snr > 0) tone else MaterialTheme.colorScheme.onSurfaceVariant,
            fontWeight = FontWeight.SemiBold
        )
    }
}

private fun sourceCaption(source: RocketSnrSummary.Source): String = when (source) {
    RocketSnrSummary.Source.DEBUG_AVERAGE ->
        "Average of active constellations (debug telemetry)."
    RocketSnrSummary.Source.DEBUG_MAX ->
        "Strongest per-constellation reading (debug telemetry)."
    RocketSnrSummary.Source.TELEMETRY_AVERAGE ->
        "Average GPS SNR byte from regular downlink."
    RocketSnrSummary.Source.NONE ->
        "Awaiting GSV sentences from rocket GPS module."
}
