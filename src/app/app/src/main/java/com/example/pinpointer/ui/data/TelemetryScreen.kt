package com.example.pinpointer.ui.data

import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.example.pinpointer.data.model.TelemetryJson
import com.example.pinpointer.ui.theme.DataTextStyle
import com.example.pinpointer.ui.tracking.TrackingViewModel
import com.patrykandpatrick.vico.compose.cartesian.CartesianChartHost
import com.patrykandpatrick.vico.compose.cartesian.axis.rememberBottom
import com.patrykandpatrick.vico.compose.cartesian.axis.rememberStart
import com.patrykandpatrick.vico.compose.cartesian.layer.rememberLineCartesianLayer
import com.patrykandpatrick.vico.compose.cartesian.rememberCartesianChart
import com.patrykandpatrick.vico.core.cartesian.axis.HorizontalAxis
import com.patrykandpatrick.vico.core.cartesian.axis.VerticalAxis
import com.patrykandpatrick.vico.core.cartesian.data.CartesianChartModelProducer
import com.patrykandpatrick.vico.core.cartesian.data.lineSeries

// ── Helpers ───────────────────────────────────────────────────────────────────

private fun flightStateName(state: Int) = when (state) {
    0 -> "Standby"; 1 -> "Motor Burn"; 2 -> "Coast"
    3 -> "Freefall"; 4 -> "Landed"; else -> "Unknown ($state)"
}

/** Chart data with actual elapsed-time x-axis values (seconds from first sample). */
private data class ChartData(
    val xValues: List<Double>,   // seconds since first sample in window
    val yValues: List<Double>,
    val droppedPackets: Int,
    val durationSec: Double      // total duration of the window in seconds
)

private fun buildChartData(
    history: List<TelemetryJson>,
    valueSelector: (TelemetryJson) -> Double
): ChartData {
    val nowMs = System.currentTimeMillis()
    val windowMs = 5L * 60 * 1_000
    val recent = history
        .filter { it.receivedAt >= nowMs - windowMs }
        .sortedBy { it.receivedAt }
    if (recent.isEmpty()) return ChartData(emptyList(), emptyList(), 0, 0.0)

    var dropped = 0
    recent.forEachIndexed { i, entry ->
        if (i > 0) {
            val gap = ((entry.sequenceNum - recent[i - 1].sequenceNum) + 65536) % 65536
            if (gap > 1) dropped += gap - 1
        }
    }

    val t0 = recent.first().receivedAt
    val xVals = recent.map { (it.receivedAt - t0) / 1_000.0 }
    val yVals = recent.map { valueSelector(it) }.filter { it.isFinite() }
    val duration = if (recent.size >= 2)
        (recent.last().receivedAt - t0) / 1_000.0 else 0.0

    return ChartData(xVals, yVals, dropped, duration)
}

// ── Screen ────────────────────────────────────────────────────────────────────

@Composable
fun DataScreen(viewModel: TrackingViewModel) {
    val uiState by viewModel.uiState.collectAsState()
    val history by viewModel.repository.history.collectAsState()
    val latest = uiState.telemetry

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            contentPadding = PaddingValues(vertical = 16.dp)
        ) {
            item {
                Text(
                    "Telemetry Data",
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(bottom = 4.dp)
                )
                Text(
                    "5-minute rolling window · tap any card to expand its chart",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.5f)
                )
            }

            item {
                MetricChartCard(
                    "Barometric Altitude", "m",
                    "%.1f m".format(latest?.altitudeM ?: 0f), history
                ) { it.altitudeM.toDouble() }
            }
            item {
                MetricChartCard(
                    "GPS Altitude", "m",
                    "%.1f m".format(latest?.gpsAltM ?: 0f), history
                ) { it.gpsAltM.toDouble() }
            }
            item {
                MetricChartCard(
                    "Vertical Velocity", "m/s",
                    "%+.1f m/s".format(latest?.velocityMps ?: 0f), history
                ) { it.velocityMps.toDouble() }
            }
            item {
                MetricChartCard(
                    "Acceleration Z", "g",
                    "%.2f g".format(latest?.accelZGs ?: 0f), history
                ) { it.accelZGs.toDouble() }
            }
            item {
                MetricChartCard(
                    "RSSI", "dBm",
                    "${latest?.rssi ?: 0} dBm", history
                ) { it.rssi.toDouble() }
            }
            item {
                MetricChartCard(
                    "Signal-to-Noise Ratio", "dB",
                    "%.1f dB".format(latest?.snr ?: 0f), history
                ) { it.snr.toDouble() }
            }
            item {
                MetricChartCard(
                    "Rocket GPS SNR", "dB-Hz",
                    if ((latest?.gpsSNR ?: 0) > 0) "${latest!!.gpsSNR} dB-Hz" else "—", history
                ) { it.gpsSNR.toDouble() }
            }

            item {
                ElevatedCard(modifier = Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(20.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column {
                            Text(
                                "Flight State", style = MaterialTheme.typography.titleSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                            Spacer(Modifier.height(4.dp))
                            Text(
                                flightStateName(latest?.flightState ?: 0), style = DataTextStyle,
                                color = MaterialTheme.colorScheme.onSurface
                            )
                        }
                        Text(
                            "Seq ${latest?.sequenceNum ?: 0}",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
            }
        }
    }
}

// ── Chart card ────────────────────────────────────────────────────────────────

@Composable
private fun MetricChartCard(
    title: String,
    unit: String,
    currentValue: String,
    history: List<TelemetryJson>,
    valueSelector: (TelemetryJson) -> Double
) {
    var expanded by remember { mutableStateOf(false) }
    val chevronRotation by animateFloatAsState(
        targetValue = if (expanded) 180f else 0f,
        animationSpec = spring(dampingRatio = 0.7f),
        label = "chevron_$title"
    )
    val modelProducer = remember { CartesianChartModelProducer() }
    var droppedCount by remember { mutableIntStateOf(0) }

    // Always update chart data when history changes so the producer is ready when the card opens.
    LaunchedEffect(history) {
        val data = buildChartData(history, valueSelector)
        droppedCount = data.droppedPackets
        if (data.yValues.size >= 2) {
            try {
                modelProducer.runTransaction {
                    lineSeries { series(data.yValues) }
                }
            } catch (_: Exception) {
            }
        }
    }

    ElevatedCard(
        modifier = Modifier
            .fillMaxWidth()
            .animateContentSize(animationSpec = spring(dampingRatio = 0.8f))
            .clickable { expanded = !expanded },
        shape = MaterialTheme.shapes.large
    ) {
        Column(modifier = Modifier.padding(horizontal = 20.dp, vertical = 16.dp)) {
            // Header
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        title, style = MaterialTheme.typography.titleSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(Modifier.height(4.dp))
                    Text(
                        currentValue, style = DataTextStyle,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (!expanded) {
                        Text(
                            unit, style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Spacer(Modifier.width(8.dp))
                    }
                    Icon(
                        Icons.Rounded.ExpandMore,
                        contentDescription = if (expanded) "Collapse" else "Expand",
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(20.dp).rotate(chevronRotation)
                    )
                }
            }

            // Chart
            if (expanded) {
                Spacer(Modifier.height(16.dp))
                // Vico 2.4.4 renders axis labels in white by default.
                // Wrapping in a dark surface makes them legible in both themes.
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(MaterialTheme.shapes.medium)
                        .background(Color(0xFF1B2022))
                ) {
                    CartesianChartHost(
                        chart = rememberCartesianChart(
                            rememberLineCartesianLayer(),
                            startAxis = VerticalAxis.rememberStart(),
                            bottomAxis = HorizontalAxis.rememberBottom(),
                        ),
                        modelProducer = modelProducer,
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(200.dp)
                    )
                }
                Spacer(Modifier.height(4.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Text(
                        "${history.size} pts · x-axis = elapsed seconds (≈0.5 s/sample)",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    if (droppedCount > 0) {
                        Text(
                            "$droppedCount dropped",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.error
                        )
                    }
                }
            }
        }
    }
}
