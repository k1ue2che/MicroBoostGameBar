
using System;
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
            new SolidColorBrush(Color.FromArgb(
                255, 38, 197, 140));

        private readonly SolidColorBrush _inactiveBrush =
            new SolidColorBrush(Color.FromArgb(
                255, 96, 105, 117));

        public MainPage()
        {
            InitializeComponent();

            _settings = ApplicationData.Current.LocalSettings;

            Loaded += MainPage_Loaded;
        }

        private void MainPage_Loaded(
            object sender,
            RoutedEventArgs e)
        {
            LoadSettings();

            BoostSlider.ValueChanged += BoostSlider_ValueChanged;

            NoiseGateToggle.Toggled += NoiseGateToggle_Toggled;

            BoostToggleButton.Click += BoostToggleButton_Click;

            UpdateBoostDisplay();
            UpdateEngineStatus();
        }

        private void LoadSettings()
        {
            // Default boost: 100%
            double boost = 100;

            if (_settings.Values.ContainsKey(BoostSetting))
            {
                var stored = _settings.Values[BoostSetting];

                if (stored is double value)
                {
                    boost = value;
                }
            }

            // Keep settings within supported limits
            boost = Math.Max(10, Math.Min(500, boost));

            BoostSlider.Value = boost;

            // Restore noise gate preference
            bool noiseGate = false;

            if (_settings.Values.ContainsKey(NoiseGateSetting))
            {
                var stored = _settings.Values[NoiseGateSetting];

                if (stored is bool value)
                {
                    noiseGate = value;
                }
            }

            NoiseGateToggle.IsOn = noiseGate;
        }

        private void BoostSlider_ValueChanged(
            object sender,
            Windows.UI.Xaml.Controls.Primitives
                .RangeBaseValueChangedEventArgs e)
        {
            UpdateBoostDisplay();

            _settings.Values[BoostSetting] =
                BoostSlider.Value;
        }

        private void UpdateBoostDisplay()
        {
            BoostValueText.Text =
                Math.Round(BoostSlider.Value).ToString();
        }

        private void NoiseGateToggle_Toggled(
            object sender,
            RoutedEventArgs e)
        {
            _settings.Values[NoiseGateSetting] =
                NoiseGateToggle.IsOn;

            UpdateEngineStatus();
        }

        private async void BoostToggleButton_Click(
            object sender,
            RoutedEventArgs e)
        {
            // Audio engine integration is not implemented yet.
            // Never report boost as active without a running engine.

            var dialog = new ContentDialog
            {
                Title = "Audio Engine Not Connected",

                Content =
                    "The microphone boost engine is not installed " +
                    "or connected yet.\n\n" +
                    "Your settings have been saved. " +
                    "Audio processing will become available " +
                    "after the MicroBoost engine has been integrated.",

                CloseButtonText = "OK"
            };

            await dialog.ShowAsync();

            UpdateEngineStatus();
        }

        private void UpdateEngineStatus()
        {
            // Real audio processing is not connected yet.

            StatusIndicator.Fill = _inactiveBrush;

            StatusText.Text = "Audio engine not connected";

            BoostToggleButton.Content = "ENABLE BOOST";

            BoostToggleButton.IsEnabled = true;

            // No simulated audio levels
            InputLevelMeter.Value = 0;

            InputLevelText.Text = "-- dBFS";
        }

        protected override void OnNavigatedFrom(
            NavigationEventArgs e)
        {
            BoostSlider.ValueChanged -= BoostSlider_ValueChanged;

            NoiseGateToggle.Toggled -= NoiseGateToggle_Toggled;

            BoostToggleButton.Click -= BoostToggleButton_Click;

            Loaded -= MainPage_Loaded;

            base.OnNavigatedFrom(e);
        }
    }
}
