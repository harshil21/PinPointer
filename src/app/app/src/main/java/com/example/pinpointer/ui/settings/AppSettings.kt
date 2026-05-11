package com.example.pinpointer.ui.settings

enum class ThemeMode {
    SYSTEM,  // Follow system / Material You dynamic colour
    LIGHT,
    DARK
}

data class AppSettings(
    val themeMode: ThemeMode = ThemeMode.SYSTEM,
    /** Force RSSI-only direction-finding even when a GPS fix is available. */
    val forceRssiOnly: Boolean = false,
    /** Survey-in minimum duration in seconds (10–600 s, default 60 s). */
    val svinDurationSeconds: Int = 120
)
