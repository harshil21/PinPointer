package com.example.pinpointer.data.remote

import com.example.pinpointer.data.model.CommandResponse
import com.example.pinpointer.data.model.StatusJson
import com.example.pinpointer.data.model.TelemetryJson
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Query

interface SopdetApi {
    @GET("/status")
    suspend fun getStatus(): StatusJson

    @GET("/telemetry/latest")
    suspend fun getLatestTelemetry(): TelemetryJson

    @GET("/telemetry/history")
    suspend fun getTelemetryHistory(@Query("limit") limit: Int = 100): List<TelemetryJson>

    @POST("/command/emergency")
    suspend fun sendEmergencyLocate(): CommandResponse

    @POST("/command/emergency/off")
    suspend fun stopEmergencyLocate(): CommandResponse

    @POST("/command/deploy")
    suspend fun sendDeployEjection(): CommandResponse

    @POST("/config/svin")
    suspend fun setSvinDuration(@retrofit2.http.Body body: com.example.pinpointer.data.model.SvinConfigBody): CommandResponse

    @POST("/resurvey")
    suspend fun requestResurvey(): CommandResponse
}
