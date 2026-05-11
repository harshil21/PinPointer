package com.example.pinpointer.util

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ConnectionPrefsTest {
    @Test
    fun acceptsIpv4Addresses() {
        assertTrue(ConnectionPrefs.isValidHostOrIp("192.168.1.100"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("10.0.0.1"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("255.255.255.255"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("0.0.0.0"))
    }

    @Test
    fun acceptsHostnames() {
        assertTrue(ConnectionPrefs.isValidHostOrIp("rtkbase"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("rtkbase.lan"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("base-station.local"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("pinpointer.example.com"))
    }

    @Test
    fun rejectsBlankAndMalformed() {
        assertFalse(ConnectionPrefs.isValidHostOrIp(""))
        assertFalse(ConnectionPrefs.isValidHostOrIp("   "))
        // Hyphen at the start of a label is invalid per RFC 1123.
        assertFalse(ConnectionPrefs.isValidHostOrIp("-leading.hyphen"))
        assertFalse(ConnectionPrefs.isValidHostOrIp("has spaces"))
        assertFalse(ConnectionPrefs.isValidHostOrIp("under_score.bad"))
    }

    @Test
    fun tolerantOfWhitespaceAndScheme() {
        assertTrue(ConnectionPrefs.isValidHostOrIp("  192.168.1.100  "))
        assertTrue(ConnectionPrefs.isValidHostOrIp("http://192.168.1.100"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("https://rtkbase.lan"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("http://rtkbase.lan/"))
    }

    @Test
    fun acceptsHostPort() {
        assertTrue(ConnectionPrefs.isValidHostOrIp("192.168.1.100:8080"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("rtkbase.lan:8080"))
        assertTrue(ConnectionPrefs.isValidHostOrIp("http://rtkbase.lan:9000"))
    }

    @Test
    fun rejectsBadPorts() {
        assertFalse(ConnectionPrefs.isValidHostOrIp("192.168.1.100:"))
        assertFalse(ConnectionPrefs.isValidHostOrIp("192.168.1.100:abc"))
        assertFalse(ConnectionPrefs.isValidHostOrIp("192.168.1.100:0"))
        assertFalse(ConnectionPrefs.isValidHostOrIp("192.168.1.100:70000"))
        // Multiple colons (e.g. unbracketed IPv6) — not supported.
        assertFalse(ConnectionPrefs.isValidHostOrIp("fe80::1"))
    }

    @Test
    fun normalizeStripsSchemeAndPath() {
        assertEquals("192.168.1.100", ConnectionPrefs.normalize("  192.168.1.100  "))
        assertEquals("192.168.1.100", ConnectionPrefs.normalize("http://192.168.1.100/"))
        assertEquals("192.168.1.100:8080", ConnectionPrefs.normalize("http://192.168.1.100:8080/status"))
        assertEquals("rtkbase.lan", ConnectionPrefs.normalize("https://rtkbase.lan"))
    }

    @Test
    fun splitHostPortHandlesBothForms() {
        assertEquals("rtkbase.lan" to null, ConnectionPrefs.splitHostPort("rtkbase.lan"))
        assertEquals("192.168.1.100" to "9000", ConnectionPrefs.splitHostPort("192.168.1.100:9000"))
        assertNull(ConnectionPrefs.splitHostPort("fe80::1"))
    }
}
