package com.example.pinpointer.ui.basestation

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Error
import androidx.compose.material.icons.rounded.GpsFixed
import androidx.compose.material.icons.rounded.GpsNotFixed
import androidx.compose.material.icons.rounded.RadioButtonUnchecked
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material.icons.rounded.SyncAlt
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material3.Button
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.example.pinpointer.data.repository.BaseStationConnectionState
import com.example.pinpointer.data.model.ConstellationSnrJson
import com.example.pinpointer.data.model.GpsFixJson
import com.example.pinpointer.data.model.StatusJson
import com.example.pinpointer.ui.theme.DataTextStyleSmall
import com.example.pinpointer.ui.tracking.TrackingViewModel
import com.example.pinpointer.util.ConnectionPrefs

@Composable
fun BaseStationScreen(
    viewModel: TrackingViewModel,
    baseStationHost: String,
    onConnect: (String) -> Unit
) {
    val uiState by viewModel.uiState.collectAsState()
    val connectionState by viewModel.repository.connectionState.collectAsState()
    val status = uiState.status
    var hostText by remember(baseStationHost) { mutableStateOf(baseStationHost) }
    val validHost = ConnectionPrefs.isValidHostOrIp(hostText)

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 16.dp)
        ) {
            item {
                Text(
                    text = "Base Station",
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(bottom = 4.dp)
                )
                Text(
                    text = "Sopdet ground station metrics",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f)
                )
            }

            item {
                ConnectionCard(
                    hostText = hostText,
                    onHostTextChange = { hostText = it },
                    validHost = validHost,
                    state = connectionState,
                    onConnect = { onConnect(hostText) }
                )
            }

            item { GpsStatusCard(status?.gpsFix, status?.gpsSNR ?: ConstellationSnrJson()) }
            item { SurveyInCard(status) }
            item { DownlinkCard(status) }
            item { SystemCard(status) }
        }
    }
}

@Composable
private fun ConnectionCard(
    hostText: String,
    onHostTextChange: (String) -> Unit,
    validHost: Boolean,
    state: BaseStationConnectionState,
    onConnect: () -> Unit
) {
    val (label, color, icon) = when (state) {
        BaseStationConnectionState.DISCONNECTED ->
            Triple("Not connected", MaterialTheme.colorScheme.outline, Icons.Rounded.Wifi)
        BaseStationConnectionState.ONLINE ->
            Triple("Online", MaterialTheme.colorScheme.primary, Icons.Rounded.CheckCircle)
        BaseStationConnectionState.SOPDET_UNAVAILABLE ->
            Triple("Host reachable, Sopdet unavailable", MaterialTheme.colorScheme.tertiary, Icons.Rounded.Error)
        BaseStationConnectionState.HOST_UNREACHABLE ->
            Triple("Base station host unreachable", MaterialTheme.colorScheme.error, Icons.Rounded.Error)
    }

    SectionCard(title = "Connection", icon = icon, iconTint = color) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            StatusDot(color)
            Spacer(modifier = Modifier.width(8.dp))
            Text(
                label,
                style = MaterialTheme.typography.bodyMedium,
                color = color,
                fontWeight = FontWeight.Medium
            )
        }
        Spacer(modifier = Modifier.height(12.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            OutlinedTextField(
                value = hostText,
                onValueChange = onHostTextChange,
                label = { Text("Base Station Host or IP") },
                isError = !validHost && hostText.isNotBlank(),
                supportingText = {
                    if (!validHost && hostText.isNotBlank()) {
                        Text("Enter host, host:port, or IPv4", color = MaterialTheme.colorScheme.error)
                    }
                },
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Uri,
                    imeAction = ImeAction.Go,
                    autoCorrectEnabled = false
                ),
                singleLine = true,
                modifier = Modifier.weight(1f)
            )
            Button(
                onClick = onConnect,
                enabled = validHost,
            ) {
                Text("Connect")
            }
        }
    }
}

@Composable
private fun SectionCard(
    title: String,
    icon: ImageVector,
    iconTint: Color = MaterialTheme.colorScheme.primary,
    content: @Composable () -> Unit
) {
    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large
    ) {
        Column(modifier = Modifier.padding(20.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.padding(bottom = 16.dp)
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = iconTint,
                    modifier = Modifier.size(20.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold
                )
            }
            content()
        }
    }
}

@Composable
private fun MetricRow(label: String, value: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Text(
            text = value,
            style = DataTextStyleSmall,
            color = MaterialTheme.colorScheme.onSurface
        )
    }
}

@Composable
private fun StatusDot(color: Color) {
    androidx.compose.foundation.Canvas(
        modifier = Modifier
            .size(10.dp)
            .clip(CircleShape)
    ) {
        drawCircle(color)
    }
}

@Composable
private fun GpsStatusCard(gpsFix: GpsFixJson?, gpsSNR: ConstellationSnrJson) {
    val fixColor = when {
        gpsFix == null -> MaterialTheme.colorScheme.error
        gpsFix.fixQuality.contains("Rtk", ignoreCase = true) -> MaterialTheme.colorScheme.primary
        gpsFix.fixQuality == "GpsFix" || gpsFix.fixQuality == "GPS" -> MaterialTheme.colorScheme.secondary
        else -> MaterialTheme.colorScheme.error
    }

    SectionCard(
        title = "Base Station GPS",
        icon = if (gpsFix != null) Icons.Rounded.GpsFixed else Icons.Rounded.GpsNotFixed,
        iconTint = fixColor
    ) {
        // ── Fix info ──────────────────────────────────────────────────────────
        if (gpsFix == null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                StatusDot(MaterialTheme.colorScheme.error)
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    "No GPS Fix", style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error
                )
            }
        } else {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.padding(bottom = 10.dp)
            ) {
                StatusDot(fixColor)
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = gpsFix.fixQuality,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.SemiBold, color = fixColor
                )
            }
            MetricRow("Latitude", "%.8f°".format(gpsFix.latitude))
            MetricRow("Longitude", "%.8f°".format(gpsFix.longitude))
            MetricRow("Altitude", "%.2f m".format(gpsFix.altitudeM))
            MetricRow("Satellites", "${gpsFix.satellitesUsed}")
            MetricRow("HDOP", "%.2f".format(gpsFix.hdop))
        }

        // ── Constellation SNR breakdown ───────────────────────────────────────
        Spacer(modifier = Modifier.height(12.dp))
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        Spacer(modifier = Modifier.height(10.dp))

        val avgColor = when {
            gpsSNR.average >= 35 -> MaterialTheme.colorScheme.primary
            gpsSNR.average >= 25 -> MaterialTheme.colorScheme.secondary
            gpsSNR.average > 0 -> MaterialTheme.colorScheme.error
            else -> MaterialTheme.colorScheme.outline
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                "GPS Signal Quality", style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold
            )
            Text(
                if (gpsSNR.hasData) "Avg ${gpsSNR.average} dB-Hz" else "No data",
                style = MaterialTheme.typography.labelMedium,
                color = avgColor, fontWeight = FontWeight.SemiBold
            )
        }
        Spacer(modifier = Modifier.height(8.dp))

        if (!gpsSNR.hasData) {
            Text(
                "Awaiting GSV sentences from GPS module…",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        } else {
            ConstellationSnrRow("GPS", gpsSNR.gps)
            ConstellationSnrRow("GLONASS", gpsSNR.glonass)
            ConstellationSnrRow("Galileo", gpsSNR.galileo)
            ConstellationSnrRow("BeiDou", gpsSNR.beidou)
            if (gpsSNR.qzss > 0)
                ConstellationSnrRow("QZSS", gpsSNR.qzss)
        }
    }
}

@Composable
private fun ConstellationSnrRow(name: String, snr: Int) {
    if (snr == 0) return
    val color = when {
        snr >= 35 -> MaterialTheme.colorScheme.primary
        snr >= 25 -> MaterialTheme.colorScheme.secondary
        else -> MaterialTheme.colorScheme.error
    }
    val quality = when {
        snr >= 35 -> "Excellent"
        snr >= 30 -> "Good"
        snr >= 25 -> "Fair"
        else -> "Poor"
    }
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp)
        ) {
            StatusDot(color)
            Text(
                name, style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                quality, style = MaterialTheme.typography.labelSmall,
                color = color
            )
            Text(
                "$snr dB-Hz", style = DataTextStyleSmall,
                color = MaterialTheme.colorScheme.onSurface
            )
        }
    }
}

@Composable
private fun SurveyInCard(status: StatusJson?) {
    val (statusText, statusColor) = when {
        status == null -> Pair("Unknown", MaterialTheme.colorScheme.outline)
        status.svinComplete -> Pair("Complete", MaterialTheme.colorScheme.primary)
        status.svinActive -> Pair("In Progress", MaterialTheme.colorScheme.tertiary)
        status.gpsFix != null -> Pair("Active (awaiting status msg)", MaterialTheme.colorScheme.secondary)
        else -> Pair("Not Started", MaterialTheme.colorScheme.outline)
    }

    val animatedColor by animateColorAsState(targetValue = statusColor, label = "svinColor")

    SectionCard(
        title = "RTK Survey-In",
        icon = Icons.Rounded.RadioButtonUnchecked,
        iconTint = animatedColor
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            StatusDot(animatedColor)
            Spacer(modifier = Modifier.width(8.dp))
            Text(
                text = statusText,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                color = animatedColor
            )
        }
        if (status != null) {
            Spacer(modifier = Modifier.height(8.dp))

            // ── Live progress (shown during AND after survey-in) ───────────
            // Elapsed / target duration
            val elapsed = status.svinElapsedS
            val target = status.svinDurationS.toLong()
            val elapsedText = if (target > 0)
                "%d / %d s".format(elapsed, target) else "%d s".format(elapsed)
            MetricRow("Elapsed", elapsedText)
            MetricRow("Min Duration", "${status.svinDurationS} s")

            // Accuracy is always meaningful: small = converged, large = still settling.
            if (status.svinAccuracyM > 0f) {
                val accuracyColor = when {
                    status.svinAccuracyM <= 5f -> MaterialTheme.colorScheme.primary
                    status.svinAccuracyM <= 15f -> MaterialTheme.colorScheme.tertiary
                    else -> MaterialTheme.colorScheme.error
                }
                Row(
                    modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        "Accuracy",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Text(
                        "%.2f m".format(status.svinAccuracyM),
                        style = DataTextStyleSmall,
                        color = accuracyColor,
                        fontWeight = FontWeight.SemiBold
                    )
                }
            }
            if (status.svinObservations > 0) {
                MetricRow("Observations", "${status.svinObservations}")
            }
        }

        if (status?.svinComplete == true) {
            Spacer(modifier = Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Rounded.CheckCircle,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(16.dp)
                )
                Spacer(modifier = Modifier.width(6.dp))
                Text(
                    "RTK corrections are being transmitted",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
    }
}

@Composable
private fun DownlinkCard(status: StatusJson?) {
    val rssi = status?.lastDownlinkRssi
    val rssiColor = when {
        rssi == null -> MaterialTheme.colorScheme.outline
        rssi > -100 -> MaterialTheme.colorScheme.primary
        rssi > -110 -> MaterialTheme.colorScheme.tertiary
        else -> MaterialTheme.colorScheme.error
    }

    SectionCard(
        title = "Downlink Reception",
        icon = Icons.Rounded.SignalCellularAlt,
        iconTint = rssiColor
    ) {
        if (rssi == null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                StatusDot(MaterialTheme.colorScheme.outline)
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    "No downlink received",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        } else {
            MetricRow("Last RSSI", "$rssi dBm")
            MetricRow(
                "Signal Quality", when {
                    rssi > -100 -> "Good"
                    rssi > -110 -> "Fair"
                    else -> "Weak"
                }
            )
        }
        MetricRow("Packets Buffered", "${status?.telemetryCount ?: 0}")
    }
}

@Composable
private fun SystemCard(status: StatusJson?) {
    SectionCard(
        title = "System",
        icon = Icons.Rounded.Schedule
    ) {
        val uptime = status?.uptimeSeconds ?: 0L
        val h = uptime / 3600
        val m = (uptime % 3600) / 60
        val s = uptime % 60

        MetricRow("Uptime", "%02d:%02d:%02d".format(h, m, s))

        Spacer(modifier = Modifier.height(8.dp))

        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                imageVector = Icons.Rounded.SyncAlt,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(16.dp)
            )
            Spacer(modifier = Modifier.width(8.dp))
            Text(
                text = "Uplink: RTK corrections (automatic)",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}
