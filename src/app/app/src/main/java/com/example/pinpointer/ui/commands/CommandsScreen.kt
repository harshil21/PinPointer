package com.example.pinpointer.ui.commands

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material.icons.rounded.BugReport
import androidx.compose.material.icons.rounded.CrisisAlert
import androidx.compose.material.icons.rounded.MyLocation
import androidx.compose.material.icons.rounded.Warning
import androidx.compose.material.icons.automirrored.rounded.VolumeOff
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.example.pinpointer.ui.tracking.TrackingViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun CommandsScreen(viewModel: TrackingViewModel) {
    val scope = rememberCoroutineScope()
    val uiState by viewModel.uiState.collectAsState()
    var showDeployDialog by remember { mutableStateOf(false) }
    var showResurveyDialog by remember { mutableStateOf(false) }

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            contentPadding = PaddingValues(vertical = 16.dp)
        ) {
            item {
                Text(
                    text = "Mission Commands",
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(bottom = 4.dp)
                )
                Text(
                    text = "Commands are transmitted to the rocket via LoRa uplink",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f)
                )
            }

            item {
                SimpleCommandCard(
                    icon = Icons.Rounded.MyLocation,
                    title = "Re-Survey Base Station",
                    description = "Restart GPS survey-in after moving the base station. Survey takes approximately 150 seconds to converge to a fixed position.",
                    buttonLabel = "Re-Survey",
                    onSend = { showResurveyDialog = true }
                )
            }

            item {
                EmergencyLocatorCard(
                    onActivate = {
                        scope.launch {
                            try {
                                viewModel.repository.sendEmergencyLocate()
                            } catch (_: Exception) {
                            }
                        }
                    },
                    onDeactivate = {
                        scope.launch {
                            try {
                                viewModel.repository.stopEmergencyLocate()
                            } catch (_: Exception) {
                            }
                        }
                    }
                )
            }

            item {
                DebugTelemetryCard(
                    enabled = uiState.status?.rocketDebugSnr != null,
                    onEnable = {
                        scope.launch {
                            try {
                                viewModel.repository.enableDebugTelemetry()
                            } catch (_: Exception) {
                            }
                        }
                    },
                    onDisable = {
                        scope.launch {
                            try {
                                viewModel.repository.disableDebugTelemetry()
                            } catch (_: Exception) {
                            }
                        }
                    }
                )
            }

            item {
                DeployCard(onDeploy = { showDeployDialog = true })
            }
        }
    }

    // Resurvey confirmation
    if (showResurveyDialog) {
        AlertDialog(
            onDismissRequest = { showResurveyDialog = false },
            icon = { Icon(Icons.Rounded.MyLocation, contentDescription = null) },
            title = { Text("Re-Survey Base Station?") },
            text = {
                Text(
                    "This will restart the GPS survey-in process. The base station will lose its fixed position until the survey converges (~150 s). Continue?",
                    style = MaterialTheme.typography.bodyMedium
                )
            },
            confirmButton = {
                Button(onClick = {
                    scope.launch {
                        try {
                            viewModel.repository.requestResurvey()
                        } catch (_: Exception) {
                        }
                    }
                    showResurveyDialog = false
                }) { Text("Re-Survey") }
            },
            dismissButton = {
                TextButton(onClick = { showResurveyDialog = false }) { Text("Cancel") }
            }
        )
    }

    // Deploy confirmation
    if (showDeployDialog) {
        AlertDialog(
            onDismissRequest = { showDeployDialog = false },
            icon = {
                Icon(
                    Icons.Rounded.Warning,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.error
                )
            },
            title = { Text("Confirm Deployment") },
            text = {
                Text(
                    "You are about to fire the ejection charge immediately, regardless of flight state. This CANNOT be undone and will end the flight. Are you absolutely certain?",
                    style = MaterialTheme.typography.bodyMedium
                )
            },
            confirmButton = {
                Button(
                    onClick = {
                        scope.launch {
                            try {
                                viewModel.repository.sendDeployEjection()
                            } catch (_: Exception) {
                            }
                        }
                        showDeployDialog = false
                    },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                        contentColor = MaterialTheme.colorScheme.onError
                    )
                ) { Text("Deploy Now") }
            },
            dismissButton = {
                TextButton(onClick = { showDeployDialog = false }) { Text("Cancel") }
            }
        )
    }
}

@Composable
private fun SimpleCommandCard(
    icon: ImageVector,
    title: String,
    description: String,
    buttonLabel: String,
    onSend: () -> Unit
) {
    var sent by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large
    ) {
        Column(modifier = Modifier.padding(20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(24.dp)
                )
                Spacer(modifier = Modifier.width(12.dp))
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold
                )
            }
            Spacer(modifier = Modifier.height(10.dp))
            Text(
                text = description,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(modifier = Modifier.height(16.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                AnimatedVisibility(visible = sent, enter = fadeIn(), exit = fadeOut()) {
                    Text(
                        "Queued ✓",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.primary,
                        fontWeight = FontWeight.SemiBold
                    )
                }
                Spacer(modifier = Modifier.weight(1f))
                FilledTonalButton(onClick = {
                    onSend()
                    sent = true
                    scope.launch { delay(2000); sent = false }
                }) {
                    Text(buttonLabel)
                }
            }
        }
    }
}

@Composable
private fun EmergencyLocatorCard(
    onActivate: () -> Unit,
    onDeactivate: () -> Unit
) {
    var activateSent by remember { mutableStateOf(false) }
    var deactivateSent by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large
    ) {
        Column(modifier = Modifier.padding(20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Rounded.CrisisAlert,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.secondary,
                    modifier = Modifier.size(24.dp)
                )
                Spacer(modifier = Modifier.width(12.dp))
                Text(
                    text = "Emergency Locator",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold
                )
            }
            Spacer(modifier = Modifier.height(10.dp))
            Text(
                text = "Activate the emergency buzzer on the rocket to help locate it after landing. Turn it off once found.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(modifier = Modifier.height(16.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                FilledTonalButton(
                    onClick = {
                        onActivate()
                        activateSent = true
                        scope.launch { delay(2000); activateSent = false }
                    },
                    modifier = Modifier.weight(1f)
                ) {
                    Icon(Icons.AutoMirrored.Rounded.VolumeUp, null, modifier = Modifier.size(16.dp).padding(end = 4.dp))
                    Text(if (activateSent) "Queued ✓" else "Activate")
                }
                OutlinedButton(
                    onClick = {
                        onDeactivate()
                        deactivateSent = true
                        scope.launch { delay(2000); deactivateSent = false }
                    },
                    modifier = Modifier.weight(1f)
                ) {
                    Icon(
                        Icons.AutoMirrored.Rounded.VolumeOff,
                        null,
                        modifier = Modifier.size(16.dp).padding(end = 4.dp)
                    )
                    Text(if (deactivateSent) "Queued ✓" else "Deactivate")
                }
            }
        }
    }
}

@Composable
private fun DebugTelemetryCard(
    enabled: Boolean,
    onEnable: () -> Unit,
    onDisable: () -> Unit
) {
    var enableSent by remember { mutableStateOf(false) }
    var disableSent by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large
    ) {
        Column(modifier = Modifier.padding(20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Rounded.BugReport,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.tertiary,
                    modifier = Modifier.size(24.dp)
                )
                Spacer(modifier = Modifier.width(12.dp))
                Text(
                    text = "Rocket Debug Telemetry",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold
                )
            }
            Spacer(modifier = Modifier.height(10.dp))
            Text(
                text = "Ask the rocket to additionally transmit per-constellation GPS " +
                        "SNR (GPS / GLONASS / Galileo / BeiDou / QZSS) once per second. " +
                        "Doubles the LoRa duty cycle while enabled.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(modifier = Modifier.height(12.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                val statusColor =
                    if (enabled) MaterialTheme.colorScheme.tertiary
                    else MaterialTheme.colorScheme.outline
                Text(
                    text = if (enabled) "Debug telemetry: ACTIVE" else "Debug telemetry: off",
                    style = MaterialTheme.typography.labelMedium,
                    color = statusColor,
                    fontWeight = FontWeight.SemiBold
                )
            }
            Spacer(modifier = Modifier.height(12.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                FilledTonalButton(
                    onClick = {
                        onEnable()
                        enableSent = true
                        scope.launch { delay(2000); enableSent = false }
                    },
                    enabled = !enabled,
                    modifier = Modifier.weight(1f)
                ) {
                    Text(if (enableSent) "Queued ✓" else "Enable")
                }
                OutlinedButton(
                    onClick = {
                        onDisable()
                        disableSent = true
                        scope.launch { delay(2000); disableSent = false }
                    },
                    enabled = enabled,
                    modifier = Modifier.weight(1f)
                ) {
                    Text(if (disableSent) "Queued ✓" else "Disable")
                }
            }
        }
    }
}

@Composable
private fun DeployCard(onDeploy: () -> Unit) {
    androidx.compose.material3.Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = androidx.compose.material3.CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer
        )
    ) {
        Column(modifier = Modifier.padding(20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Rounded.Warning,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.error,
                    modifier = Modifier.size(24.dp)
                )
                Spacer(modifier = Modifier.width(12.dp))
                Text(
                    text = "Deploy Ejection Charge",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.error
                )
            }
            Spacer(modifier = Modifier.height(10.dp))
            Text(
                text = "DANGER: Immediately fires the ejection charge regardless of flight state. This action cannot be undone and will end the flight.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onErrorContainer
            )
            Spacer(modifier = Modifier.height(16.dp))
            Button(
                onClick = onDeploy,
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.buttonColors(
                    containerColor = MaterialTheme.colorScheme.error,
                    contentColor = MaterialTheme.colorScheme.onError
                ),
                shape = MaterialTheme.shapes.large
            ) {
                Icon(Icons.Rounded.Warning, null, modifier = Modifier.size(18.dp))
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    "Deploy Now",
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold
                )
            }
        }
    }
}
