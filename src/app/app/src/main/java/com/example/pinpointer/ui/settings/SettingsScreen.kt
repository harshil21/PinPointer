package com.example.pinpointer.ui.settings

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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Brightness4
import androidx.compose.material.icons.rounded.Brightness7
import androidx.compose.material.icons.rounded.BrightnessAuto
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp

@Composable
fun SettingsScreen(
    settings: AppSettings,
    onSettingsChange: (AppSettings) -> Unit,
    onSvinApply: (Int) -> Unit = {},
    onTxPowerApply: (Int) -> Unit = {}
) {
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
                    text = "Settings",
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(bottom = 4.dp)
                )
            }

            // ── Appearance ───────────────────────────────────────────────────────
            item {
                SettingsSectionCard(title = "Appearance") {
                    Text(
                        "Theme",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(Modifier.height(10.dp))

                    val options = listOf(
                        Triple(ThemeMode.SYSTEM, "System", Icons.Rounded.BrightnessAuto),
                        Triple(ThemeMode.LIGHT, "Light", Icons.Rounded.Brightness7),
                        Triple(ThemeMode.DARK, "Dark", Icons.Rounded.Brightness4),
                    )

                    SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
                        options.forEachIndexed { index, (mode, label, icon) ->
                            SegmentedButton(
                                shape = SegmentedButtonDefaults.itemShape(
                                    index = index, count = options.size
                                ),
                                onClick = { onSettingsChange(settings.copy(themeMode = mode)) },
                                selected = settings.themeMode == mode,
                                icon = {
                                    Icon(
                                        imageVector = icon,
                                        contentDescription = null,
                                        modifier = Modifier.size(18.dp)
                                    )
                                }
                            ) {
                                Text(label, style = MaterialTheme.typography.labelMedium)
                            }
                        }
                    }

                    Spacer(Modifier.height(6.dp))
                    Text(
                        when (settings.themeMode) {
                            ThemeMode.SYSTEM -> "Follows your system setting. Material You dynamic colour is active on Android 12+."
                            ThemeMode.LIGHT -> "Always use light theme with the PinPointer aerospace palette."
                            ThemeMode.DARK -> "Always use dark theme with the PinPointer aerospace palette."
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }

            // ── Tracking ──────────────────────────────────────────────────
            item {
                SettingsSectionCard(title = "Tracking") {
                    SwitchSettingRow(
                        icon = Icons.Rounded.SignalCellularAlt,
                        title = "Force RSSI-Only Mode",
                        description = "Use RSSI signal-strength scanning to estimate rocket direction " +
                                "even when a GPS fix is available. Useful for range testing.",
                        checked = settings.forceRssiOnly,
                        onCheckedChange = { onSettingsChange(settings.copy(forceRssiOnly = it)) }
                    )
                    Spacer(Modifier.height(18.dp))
                    TxPowerRow(
                        current = settings.txPowerDbm,
                        onApply = { dbm ->
                            onSettingsChange(settings.copy(txPowerDbm = dbm))
                            onTxPowerApply(dbm)
                        }
                    )
                }
            }

            // ── Survey-In ──────────────────────────────────────────────────
            item {
                SvinDurationCard(
                    current = settings.svinDurationSeconds,
                    onApply = { newDuration ->
                        onSettingsChange(settings.copy(svinDurationSeconds = newDuration))
                        onSvinApply(newDuration)
                    }
                )
            }
        }
    }
}

@Composable
private fun TxPowerRow(current: Int, onApply: (Int) -> Unit) {
    var text by remember(current) { mutableStateOf(current.toString()) }
    val parsed = text.toIntOrNull()
    val valid = parsed != null && parsed in 2..20

    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Icon(
            imageVector = Icons.Rounded.SignalCellularAlt,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(top = 12.dp)
        )
        Column(modifier = Modifier.weight(1f)) {
            Text("TX Power", style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
            Spacer(Modifier.height(2.dp))
            Text(
                "Sets Sopdet and Sirius LoRa output power and the Friis RSSI distance model.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(Modifier.height(10.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it.filter { c -> c.isDigit() }.take(2) },
                    label = { Text("dBm") },
                    isError = !valid && text.isNotEmpty(),
                    supportingText = {
                        if (!valid && text.isNotEmpty()) {
                            Text("Use 2-20 dBm", color = MaterialTheme.colorScheme.error)
                        }
                    },
                    keyboardOptions = KeyboardOptions(
                        keyboardType = KeyboardType.Number,
                        imeAction = ImeAction.Done
                    ),
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                    shape = MaterialTheme.shapes.large,
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = MaterialTheme.colorScheme.primary
                    )
                )
                androidx.compose.material3.FilledTonalButton(
                    onClick = { parsed?.let(onApply) },
                    enabled = valid,
                    shape = MaterialTheme.shapes.large
                ) { Text("Apply") }
            }
        }
    }
}

@Composable
private fun SvinDurationCard(current: Int, onApply: (Int) -> Unit) {
    var text by remember(current) { mutableStateOf(current.toString()) }
    val parsed = text.toIntOrNull()
    val valid = parsed != null && parsed in 10..600

    SettingsSectionCard(title = "RTK Survey-In") {
        Text(
            "Minimum duration the base station must observe satellites before " +
                    "the survey-in converges.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Spacer(Modifier.height(12.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it.filter { c -> c.isDigit() }.take(3) },
                label = { Text("Duration (s)") },
                isError = !valid && text.isNotEmpty(),
                supportingText = {
                    if (!valid && text.isNotEmpty())
                        Text("Enter time (s)", color = MaterialTheme.colorScheme.error)
                },
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Number,
                    imeAction = ImeAction.Done
                ),
                singleLine = true,
                modifier = Modifier.weight(1f),
                shape = MaterialTheme.shapes.large,
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = MaterialTheme.colorScheme.primary
                )
            )
            androidx.compose.material3.FilledTonalButton(
                onClick = { parsed?.let(onApply) },
                enabled = valid,
                shape = MaterialTheme.shapes.large
            ) { Text("Apply") }
        }
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────────

@Composable
private fun SettingsSectionCard(
    title: String,
    content: @Composable () -> Unit
) {
    ElevatedCard(modifier = Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large) {
        Column(modifier = Modifier.padding(20.dp)) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(bottom = 16.dp)
            )
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            Spacer(Modifier.height(16.dp))
            content()
        }
    }
}

@Composable
private fun SwitchSettingRow(
    icon: ImageVector,
    title: String,
    description: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(top = 2.dp)
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
            Spacer(Modifier.height(2.dp))
            Text(
                description, style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}
