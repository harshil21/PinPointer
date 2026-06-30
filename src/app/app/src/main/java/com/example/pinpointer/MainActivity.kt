package com.example.pinpointer

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.FlightTakeoff
import androidx.compose.material.icons.rounded.Router
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Terminal
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.runtime.rememberCoroutineScope
import com.example.pinpointer.data.remote.SopdetApi
import com.example.pinpointer.data.repository.TelemetryRepository
import com.example.pinpointer.ui.basestation.BaseStationScreen
import com.example.pinpointer.ui.commands.CommandsScreen
import com.example.pinpointer.ui.connect.baseUrlFor
import com.example.pinpointer.ui.settings.AppSettings
import com.example.pinpointer.ui.settings.SettingsScreen
import com.example.pinpointer.ui.theme.PinPointerTheme
import com.example.pinpointer.ui.tracking.TrackingScreen
import com.example.pinpointer.ui.tracking.TrackingViewModel
import com.example.pinpointer.util.ConnectionPrefs
import com.example.pinpointer.util.OrientationTracker
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import retrofit2.converter.gson.GsonConverterFactory
import java.util.concurrent.TimeUnit

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            var settings by remember { mutableStateOf(AppSettings()) }
            PinPointerTheme(themeMode = settings.themeMode) {
                MainApp(settings = settings, onSettingsChange = { settings = it })
            }
        }
    }
}

@Composable
fun MainApp(settings: AppSettings, onSettingsChange: (AppSettings) -> Unit) {
    val context = LocalContext.current
    val prefs = remember(context) { ConnectionPrefs(context) }
    var baseStationHost by remember {
        mutableStateOf(prefs.lastHost() ?: "192.168.1.100")
    }

    AppNavigation(
        ip = baseStationHost,
        settings = settings,
        onSettingsChange = onSettingsChange,
        onBaseStationHostChange = { host ->
            val normalized = ConnectionPrefs.normalize(host)
            if (ConnectionPrefs.isValidHostOrIp(normalized)) {
                prefs.saveHost(normalized)
                baseStationHost = normalized
            }
        }
    )
}

private data class NavDestination(val label: String, val icon: ImageVector)

@Composable
fun AppNavigation(
    ip: String,
    settings: AppSettings,
    onSettingsChange: (AppSettings) -> Unit,
    onBaseStationHostChange: (String) -> Unit
) {
    val context = LocalContext.current

    val api = remember(ip) {
        Retrofit.Builder()
            .baseUrl(baseUrlFor(ip))
            .addConverterFactory(GsonConverterFactory.create())
            .client(
                OkHttpClient.Builder()
                    .connectTimeout(3, TimeUnit.SECONDS)
                    .readTimeout(3, TimeUnit.SECONDS)
                    .build()
            )
            .build()
            .create(SopdetApi::class.java)
    }

    val repository = remember(api, ip) { TelemetryRepository(api, ip) }
    val orientationTracker = remember(context) { OrientationTracker(context) }
    DisposableEffect(orientationTracker) {
        orientationTracker.start()
        onDispose { orientationTracker.stop() }
    }
    val viewModel = remember(repository, orientationTracker) {
        TrackingViewModel(repository, orientationTracker)
    }
    DisposableEffect(repository) {
        onDispose { repository.stopPolling() }
    }

    // Keep the ViewModel's forceRssiOnly in sync with settings
    LaunchedEffect(settings.forceRssiOnly) {
        viewModel.setForceRssiOnly(settings.forceRssiOnly)
    }

    LaunchedEffect(settings.txPowerDbm) {
        viewModel.setTxPowerDbm(settings.txPowerDbm)
    }

    val destinations = listOf(
        NavDestination("Tracking", Icons.Rounded.FlightTakeoff),
        NavDestination("Commands", Icons.Rounded.Terminal),
        NavDestination("Base Station", Icons.Rounded.Router),
        NavDestination("Settings", Icons.Rounded.Settings)
    )

    var selectedTab by remember { mutableIntStateOf(0) }

    val scope = rememberCoroutineScope()

    Scaffold(
        bottomBar = {
            NavigationBar {
                destinations.forEachIndexed { index, dest ->
                    NavigationBarItem(
                        selected = selectedTab == index,
                        onClick = { selectedTab = index },
                        icon = { Icon(dest.icon, contentDescription = dest.label) },
                        label = {
                            Text(
                                text = dest.label,
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = if (selectedTab == index) FontWeight.SemiBold
                                else FontWeight.Normal
                            )
                        }
                    )
                }
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .padding(innerPadding)
                .fillMaxSize()
        ) {
            when (selectedTab) {
                0 -> TrackingScreen(viewModel)
                1 -> CommandsScreen(viewModel)
                2 -> BaseStationScreen(
                    viewModel = viewModel,
                    baseStationHost = ip,
                    onConnect = onBaseStationHostChange
                )
                3 -> SettingsScreen(
                    settings = settings,
                    onSettingsChange = onSettingsChange,
                    onSvinApply = { duration ->
                        scope.launch {
                            try {
                                viewModel.repository.setSvinDuration(duration)
                            } catch (_: Exception) {
                            }
                        }
                    },
                    onTxPowerApply = { dbm ->
                        scope.launch {
                            try {
                                viewModel.repository.setTxPower(dbm)
                            } catch (_: Exception) {
                            }
                        }
                    }
                )
            }
        }
    }
}
