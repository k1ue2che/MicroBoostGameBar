
using System;
using Microsoft.Gaming.XboxGameBar;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Navigation;

namespace MicroBoostGameBar
{
    public sealed partial class App : Application
    {
        private XboxGameBarWidget _widget;

        public App()
        {
            InitializeComponent();
            Suspending += OnSuspending;
        }

        protected override void OnActivated(IActivatedEventArgs args)
        {
            if (args.Kind == ActivationKind.Protocol)
            {
                var widgetArgs =
                    args as XboxGameBarWidgetActivatedEventArgs;

                if (widgetArgs != null &&
                    widgetArgs.AppExtensionId == "MicroBoostWidget")
                {
                    var frame = new Frame();

                    frame.NavigationFailed += OnNavigationFailed;

                    Window.Current.Content = frame;

                    _widget = new XboxGameBarWidget(
                        widgetArgs,
                        Window.Current.CoreWindow,
                        frame);

                    frame.Navigate(typeof(MainPage), _widget);

                    Window.Current.Activate();
                    return;
                }
            }

            base.OnActivated(args);
        }

        protected override void OnLaunched(LaunchActivatedEventArgs e)
        {
            if (!(Window.Current.Content is Frame rootFrame))
            {
                rootFrame = new Frame();
                rootFrame.NavigationFailed += OnNavigationFailed;
                Window.Current.Content = rootFrame;
            }

            if (!e.PrelaunchActivated)
            {
                if (rootFrame.Content == null)
                {
                    rootFrame.Navigate(
                        typeof(MainPage),
                        e.Arguments);
                }

                Window.Current.Activate();
            }
        }

        private void OnNavigationFailed(
            object sender,
            NavigationFailedEventArgs e)
        {
            throw new Exception(
                $"Failed to load page '{e.SourcePageType.FullName}'.");
        }

        private void OnSuspending(
            object sender,
            SuspendingEventArgs e)
        {
            var deferral =
                e.SuspendingOperation.GetDeferral();

            deferral.Complete();
        }
    }
}
