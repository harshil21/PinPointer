package com.example.pinpointer.ui.connect

import org.junit.Assert.assertEquals
import org.junit.Test

class BaseUrlTest {
    @Test
    fun defaultsPortTo8080() {
        assertEquals("http://192.168.1.100:8080/", baseUrlFor("192.168.1.100"))
        assertEquals("http://rtkbase.lan:8080/", baseUrlFor("rtkbase.lan"))
    }

    @Test
    fun preservesExplicitPort() {
        assertEquals("http://192.168.1.100:9000/", baseUrlFor("192.168.1.100:9000"))
        assertEquals("http://rtkbase.lan:9000/", baseUrlFor("rtkbase.lan:9000"))
    }
}
