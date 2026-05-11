package com.example.pinpointer.util

import android.content.Context
import android.content.SharedPreferences

/**
 * Persists the most-recently entered base-station host (IPv4 address or DNS
 * hostname) so the login screen can pre-fill it on subsequent launches.
 */
class ConnectionPrefs(context: Context) {
    private val prefs: SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    fun lastHost(): String? = prefs.getString(KEY_LAST_HOST, null)?.takeIf { it.isNotBlank() }

    fun saveHost(host: String) {
        prefs.edit().putString(KEY_LAST_HOST, host).apply()
    }

    companion object {
        private const val PREFS_NAME = "pinpointer_connection"
        private const val KEY_LAST_HOST = "last_host"

        // IPv4: four 0-255 octets separated by dots.
        private val IPV4 =
            Regex("^((25[0-5]|2[0-4]\\d|[01]?\\d?\\d)\\.){3}(25[0-5]|2[0-4]\\d|[01]?\\d?\\d)$")

        // Hostname per RFC 1123: labels of letters/digits/hyphens (not starting
        // or ending with a hyphen), separated by dots. Single-label hostnames
        // like "rtkbase" are accepted.
        private val HOSTNAME = Regex(
            "^(?=.{1,253}$)([A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?)(\\.[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$"
        )

        fun isValidHostOrIp(input: String): Boolean {
            if (input.isBlank()) return false
            return IPV4.matches(input) || HOSTNAME.matches(input)
        }
    }
}
