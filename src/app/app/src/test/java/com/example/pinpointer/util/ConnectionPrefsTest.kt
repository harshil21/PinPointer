package com.example.pinpointer.util

import org.junit.Assert.assertFalse
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
}
