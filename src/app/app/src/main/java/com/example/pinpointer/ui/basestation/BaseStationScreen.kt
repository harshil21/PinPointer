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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.GpsFixed
import androidx.compose.material.icons.rounded.GpsNotFixed
import androidx.compose.material.icons.rounded.RadioButtonUnchecked
import androidx.compose.material.icons.rounded.Router
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material.icons.rounded.SyncAlt
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.example.pinpointer.data.model.GpsFixJson
import com.example.pinpointer.data.model.StatusJson
import com.example.pinpointer.ui.theme.DataTextStyle
import com.example.pinpointer.ui.theme.DataTextStyleSmall
import com.example.pinpointer.ui.tracking.TrackingViewModel

@Composable
fun BaseStationScreen(viewModel: TrackingViewModel) {
    val uiState by viewModel.uiState.collectAsState()
    val status = uiState.status

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

            item { GpsStatusCard(status?.gpsFix) }
            item { SurveyInCard(status) }
            item { DownlinkCard(status) }
            item { GpsQualityCard(status?.gpsSNR ?: 0) }
            item { SystemCard(status) }
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
private fun GpsStatusCard(gpsFix: GpsFixJson?) {
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
        if (gpsFix == null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                StatusDot(MaterialTheme.colorScheme.error)
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    "No GPS Fix",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error
                )
            }
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(bottom = 12.dp)) {
                StatusDot(fixColor)
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = gpsFix.fixQuality,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.SemiBold,
                    color = fixColor
                )
            }
            MetricRow("Latitude", "%.6f°".format(gpsFix.latitude))
            MetricRow("Longitude", "%.6f°".format(gpsFix.longitude))
            MetricRow("Altitude", "%.1f m".format(gpsFix.altitudeM))
            MetricRow("Satellites", "${gpsFix.satellitesUsed}")
            MetricRow("HDOP", "%.2f".format(gpsFix.hdop))
        }
    }
}

@Composable
private fun SurveyInCard(status: StatusJson?) {
    val (statusText, statusColor) = when {
        status == null -> Pair("Unknown", MaterialTheme.colorScheme.outline)
        status.svinComplete -> Pair("Complete", MaterialTheme.colorScheme.primary)
        status.svinActive -> Pair("In Progress", MaterialTheme.colorScheme.tertiary)
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
private fun GpsQualityCard(gpsSNR: Int) {
    val quality = when {
        gpsSNR >= 35 -> Triple("Excellent", MaterialTheme.colorScheme.primary, "≥ 35 dB-Hz")
        gpsSNR >= 25 -> Triple("Good", MaterialTheme.colorScheme.secondary, "25–35 dB-Hz")
        gpsSNR > 0 -> Triple("Poor", MaterialTheme.colorScheme.error, "< 25 dB-Hz")
        else -> Triple("No data", MaterialTheme.colorScheme.outline, "–")
    }

    SectionCard(
        title = "Base Station GPS SNR",
        icon = Icons.Rounded.Router,
        iconTint = quality.second
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column {
                Text(
                    text = if (gpsSNR > 0) "$gpsSNR dB-Hz" else "—",
                    style = DataTextStyle,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Text(
                    text = quality.first,
                    style = MaterialTheme.typography.labelMedium,
                    color = quality.second,
                    fontWeight = FontWeight.SemiBold
                )
            }
            Text(
                text = quality.third,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
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
