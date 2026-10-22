
using System;
using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Threading.Tasks;

namespace MicroBoostGameBar
{
    internal static class PipeConnectionProbe
    {
        private const string PipeName =
            @"LOCAL\MicroBoostGameBar";

        public static Task<string> SendCommandAsync(
            string command)
        {
            if (string.IsNullOrWhiteSpace(command))
            {
                throw new ArgumentException(
                    "Command must not be empty.");
            }

            return Task.Run(() =>
            {
                using (var client = new NamedPipeClientStream(
                    ".",
                    PipeName,
                    PipeDirection.InOut))
                {
                    // Connect to the running Rust engine.
                    client.Connect(3000);

                    // The Rust protocol uses newline-terminated
                    // UTF-8 commands.
                    byte[] request = Encoding.UTF8.GetBytes(
                        command.Trim() + "\n");

                    client.Write(
                        request,
                        0,
                        request.Length);

                    client.Flush();

                    byte[] responseBuffer = new byte[256];

                    int count = client.Read(
                        responseBuffer,
                        0,
                        responseBuffer.Length);

                    if (count <= 0)
                    {
                        throw new IOException(
                            "The engine closed the pipe " +
                            "without a response.");
                    }

                    return Encoding.UTF8.GetString(
                        responseBuffer,
                        0,
                        count).Trim();
                }
            });
        }

        public static Task<string> PingAsync()
        {
            return SendCommandAsync("PING");
        }

        public static Task<string> SetGainAsync(int percent)
        {
            if (percent < 10 || percent > 500)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(percent));
            }

            return SendCommandAsync(
                "GAIN " + percent);
        }

        public static Task<string> GetGainAsync()
        {
            return SendCommandAsync("GET_GAIN");
        }
    }
}
