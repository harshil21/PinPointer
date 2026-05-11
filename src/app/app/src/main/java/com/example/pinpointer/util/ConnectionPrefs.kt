package com.example.pinpointer.util

import android.content.Context
import android.content.SharedPreferences

/**
 * Persists the most-recently entered base-station host (IPv4 address or DNS
 * hostname, optionally with a `:port` suffix) so the login screen can pre-fill
 * it on subsequent launches.
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

        /**
         * Strip a leading `http://` or `https://` scheme, surrounding whitespace,
         * and a trailing slash/path. Returns the raw `host[:port]` token.
         */
        fun normalize(input: String): String {
            var s = input.trim()
            s = s.removePrefix("http://").removePrefix("https://")
            val slash = s.indexOf('/')
            if (slash >= 0) s = s.substring(0, slash)
            return s
        }

        /**
         * Accept `host`, `host:port`, IPv4, or IPv4:port. Scheme prefix and a
         * trailing path are stripped before validation.
         */
        fun isValidHostOrIp(input: String): Boolean {
            val normalized = normalize(input)
            if (normalized.isBlank()) return false
            val (host, portPart) = splitHostPort(normalized) ?: return false
            if (portPart != null) {
                val port = portPart.toIntOrNull() ?: return false
                if (port !in 1..65535) return false
            }
            return IPV4.matches(host) || HOSTNAME.matches(host)
        }

        /**
         * Split `host:port` into (host, port?). Returns null if the input has
         * multiple unbracketed colons (e.g. malformed IPv6 — not supported).
         */
        fun splitHostPort(input: String): Pair<String, String?>? {
            val colon = input.indexOf(':')
            if (colon < 0) return input to null
            if (input.indexOf(':', colon + 1) >= 0) return null
            val host = input.substring(0, colon)
            val port = input.substring(colon + 1)
            if (host.isEmpty() || port.isEmpty()) return null
            return host to port
        }
    }
}
