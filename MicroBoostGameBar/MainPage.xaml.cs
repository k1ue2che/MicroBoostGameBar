
using System;
using System.IO;
using System.Threading.Tasks;
using Windows.ApplicationModel;
using Windows.Foundation.Metadata;
using Windows.Storage;
using Windows.UI;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Navigation;

namespace MicroBoostGameBar
{
    public sealed partial class MainPage : Page
    {
        private const string BoostSetting = "BoostValue";
        private const string NoiseGateSetting = "NoiseGateEnabled";

        private readonly ApplicationDataContainer _settings;

        private readonly SolidColorBrush _accentBrush =
            new SolidColorBrush(
                Color.FromArgb(255, 38, 197, 140));

        private readonly SolidColorBrush _inactiveBrush =
            new SolidColorBrush(
                Color.FromArgb(255, 96, 105, 117));

        private DispatcherTimer _boostTimer;

        private bool _eventsAttached;
        private bool _engineConnected;
        private bool _requestInFlight;

        private int _lastConfirmedGain = 100;

        private string _engineStatus =
            "Audio engine not connected";

        public MainPage()
        {
            InitializeComponent();

            _settings = ApplicationData.Current.LocalSettings;

            Loaded += MainPage_Loaded;
        }

        private async void MainPage_Loaded(
            object sender,
            RoutedEventArgs e)
        {
            if (_boostTimer == null)
            {
                _boostTimer = new DispatcherTimer();

                _boostTimer.Interval =
                    TimeSpan.FromMilliseconds(160);

                _boostTimer.Tick += BoostTimer_Tick;
            }

            LoadSettings();

            if (!_eventsAttached)
            {
                BoostSlider.ValueChanged +=
                    BoostSlider_ValueChanged;

                NoiseGateToggle.Toggled +=
                    NoiseGateToggle_Toggled;

                BoostToggleButton.Click +=
                    BoostToggleButton_Click;

                _eventsAttached = true;
            }

            UpdateBoostDisplay();
            UpdateEngineStatus();

            // Automatically connect or launch the engine.
            await ConnectEngineAsync(false);
        }

        private void LoadSettings()
        {
            double boost = 100;

            if (_settings.Values.ContainsKey(BoostSetting))
            {
                var stored = _settings.Values[BoostSetting];

                if (stored is double value)
                {
                    boost = value;
                }
            }

            BoostSlider.Value =
                Math.Max(10, Math.Min(500, boost));

            bool noiseGate = false;

            if (_settings.Values.ContainsKey(NoiseGateSetting))
            {
                var stored =
                    _settings.Values[NoiseGateSetting];

                if (stored is bool value)
                {
                    noiseGate = value;
                }
            }

            NoiseGateToggle.IsOn = noiseGate;
        }

        private int CurrentBoost
        {
            get
            {
                return (int)Math.Round(BoostSlider.Value);
            }
        }

        private void BoostSlider_ValueChanged(
            object sender,
            Windows.UI.Xaml.Controls.Primitives
                .RangeBaseValueChangedEventArgs e)
        {
            UpdateBoostDisplay();

            _settings.Values[BoostSetting] =
                BoostSlider.Value;

            if (_engineConnected)
            {
                ScheduleBoostUpdate();
            }
        }

        private void UpdateBoostDisplay()
        {
            BoostValueText.Text = CurrentBoost.ToString();
        }

        private void ScheduleBoostUpdate()
        {
            if (_boostTimer == null)
            {
                return;
            }

            _boostTimer.Stop();
            _boostTimer.Start();
        }

        private async void BoostTimer_Tick(
            object sender,
            object e)
        {
            _boostTimer.Stop();

            if (!_engineConnected)
            {
                return;
            }

            if (_requestInFlight)
            {
                ScheduleBoostUpdate();
                return;
            }

            int requestedGain = CurrentBoost;

            if (requestedGain == _lastConfirmedGain)
            {
                return;
            }

            _requestInFlight = true;
            UpdateEngineStatus();

            try
            {
                string response =
                    await PipeConnectionProbe.SetGainAsync(
                        requestedGain);

                if (response != "OK " + requestedGain)
                {
                    throw new IOException(
                        "Unexpected engine response: " +
                        response);
                }

                _lastConfirmedGain = requestedGain;

                _engineStatus =
                    "Audio engine connected - Boost " +
                    requestedGain + "%";
            }
            catch (Exception ex)
            {
                _engineConnected = false;

                _engineStatus =
                    "Connection lost: " +
                    ex.GetType().Name;
            }
            finally
            {
                _requestInFlight = false;

                UpdateEngineStatus();

                if (_engineConnected &&
                    CurrentBoost != _lastConfirmedGain)
                {
                    ScheduleBoostUpdate();
                }
            }
        }

        private void NoiseGateToggle_Toggled(
            object sender,
            RoutedEventArgs e)
        {
            // Noise gate integration comes later.
            _settings.Values[NoiseGateSetting] =
                NoiseGateToggle.IsOn;
        }

        private async Task<bool> TryPingAsync()
        {
            try
            {
                string response =
                    await PipeConnectionProbe.PingAsync();

                return response == "PONG";
            }
            catch (TimeoutException)
            {
                return false;
            }
            catch (IOException)
            {
                return false;
            }
        }

        private async Task ConnectEngineAsync(
            bool showErrorDialog)
        {
            if (_requestInFlight)
            {
                return;
            }

            if (_boostTimer != null)
            {
                _boostTimer.Stop();
            }

            _requestInFlight = true;
            _engineConnected = false;

            _engineStatus = "Checking audio engine...";

            UpdateEngineStatus();

            try
            {
                // Avoid launching another engine if one
                // is already responding.
                bool engineRunning = await TryPingAsync();

                if (!engineRunning)
                {
                    _engineStatus = "Starting audio engine...";
                    UpdateEngineStatus();

                    if (!ApiInformation.IsApiContractPresent(
                        "Windows.ApplicationModel.FullTrustAppContract",
                        1,
                        0))
                    {
                        throw new NotSupportedException(
                            "Full-trust process launching " +
                            "is not available.");
                    }

                    // Starts the EXE defined in
                    // Package.appxmanifest.
                    await FullTrustProcessLauncher
                        .LaunchFullTrustProcessForCurrentAppAsync();

                    // Wait for the Rust engine to initialize
                    // microphone, VB-CABLE and its named pipe.
                    bool ready = false;

                    for (int attempt = 0; attempt < 8; attempt++)
                    {
                        await Task.Delay(350);

                        if (await TryPingAsync())
                        {
                            ready = true;
                            break;
                        }
                    }

                    if (!ready)
                    {
                        throw new IOException(
                            "The audio engine was launched " +
                            "but its named pipe is unavailable.");
                    }
                }

                // Synchronize the saved slider value.
                int requestedGain = CurrentBoost;

                string response =
                    await PipeConnectionProbe.SetGainAsync(
                        requestedGain);

                if (response != "OK " + requestedGain)
                {
                    throw new IOException(
                        "Boost command failed: " + response);
                }

                _lastConfirmedGain = requestedGain;
                _engineConnected = true;

                _engineStatus =
                    "Audio engine connected - Boost " +
                    requestedGain + "%";
            }
            catch (Exception ex)
            {
                _engineConnected = false;

                _engineStatus =
                    "Engine connection failed: " +
                    ex.GetType().Name;

                if (showErrorDialog)
                {
                    var dialog = new ContentDialog
                    {
                        Title = "Audio Engine Startup Failed",

                        Content =
                            ex.GetType().Name + ":\n" +
                            ex.Message,

                        CloseButtonText = "OK"
                    };

                    await dialog.ShowAsync();
                }
            }
            finally
            {
                _requestInFlight = false;
                UpdateEngineStatus();

                if (_engineConnected &&
                    CurrentBoost != _lastConfirmedGain)
                {
                    ScheduleBoostUpdate();
                }
            }
        }

        private async void BoostToggleButton_Click(
            object sender,
            RoutedEventArgs e)
        {
            await ConnectEngineAsync(true);
        }

        private void UpdateEngineStatus()
        {
            StatusIndicator.Fill =
                _engineConnected
                    ? _accentBrush
                    : _inactiveBrush;

            StatusText.Text = _engineStatus;

            if (_requestInFlight)
            {
                BoostToggleButton.Content = "CONNECTING...";
                BoostToggleButton.IsEnabled = false;
            }
            else
            {
                BoostToggleButton.Content =
                    _engineConnected
                        ? "RECONNECT ENGINE"
                        : "CONNECT ENGINE";

                BoostToggleButton.IsEnabled = true;
            }

            // Real-time level metering is not ready yet.
            InputLevelMeter.Value = 0;
            InputLevelText.Text = "-- dBFS";
        }

        protected override void OnNavigatedFrom(
            NavigationEventArgs e)
        {
            if (_boostTimer != null)
            {
                _boostTimer.Stop();
                _boostTimer.Tick -= BoostTimer_Tick;
                _boostTimer = null;
            }

            if (_eventsAttached)
            {
                BoostSlider.ValueChanged -=
                    BoostSlider_ValueChanged;

                NoiseGateToggle.Toggled -=
                    NoiseGateToggle_Toggled;

                BoostToggleButton.Click -=
                    BoostToggleButton_Click;

                _eventsAttached = false;
            }

            Loaded -= MainPage_Loaded;

            base.OnNavigatedFrom(e);
        }
    }
}
