
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use crossbeam_queue::ArrayQueue;

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

const BUFFER_FRAMES: usize = 4_800;
const START_FRAMES: usize = 960;

const INPUT_NAME: &str = "DGM20 USB Microphone";
const OUTPUT_NAME: &str =
    "CABLE Input (VB-Audio Virtual Cable)";

pub struct AudioPipeline {
    _input_stream: Stream,
    _output_stream: Stream,
    gain_percent: Arc<AtomicU32>,
}

impl AudioPipeline {
    pub fn start() -> Result<Self, Box<dyn Error>> {
        let host = cpal::default_host();

        let input_device = host
            .input_devices()?
            .find(|device| {
                device.name()
                    .map(|n| n.contains(INPUT_NAME))
                    .unwrap_or(false)
            })
            .ok_or_else(|| IoError::new(
                ErrorKind::NotFound,
                "Microphone not found"
            ))?;

        let output_device = host
            .output_devices()?
            .find(|device| {
                device.name()
                    .map(|n| n == OUTPUT_NAME)
                    .unwrap_or(false)
            })
            .ok_or_else(|| IoError::new(
                ErrorKind::NotFound,
                "VB-CABLE Input not found"
            ))?;

        println!("Input: {}", input_device.name()?);
        println!("Output: {}", output_device.name()?);

        let input_supported =
            input_device.default_input_config()?;

        let output_supported =
            output_device.default_output_config()?;

        for (name, config) in [
            ("Input", &input_supported),
            ("Output", &output_supported),
        ] {
            if config.sample_rate().0 != SAMPLE_RATE
                || config.channels() != CHANNELS
                || config.sample_format() != SampleFormat::F32
            {
                return Err(IoError::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "{} must use 48000 Hz, stereo, F32",
                        name
                    ),
                ).into());
            }
        }

        let input_config = input_supported.config();
        let output_config = output_supported.config();

        let buffer: Arc<ArrayQueue<[f32; 2]>> =
            Arc::new(ArrayQueue::new(BUFFER_FRAMES));

        let input_buffer = Arc::clone(&buffer);
        let output_buffer = Arc::clone(&buffer);

        // Shared, thread-safe gain control
        let gain_percent = Arc::new(AtomicU32::new(100));
        let output_gain = Arc::clone(&gain_percent);

        // Microphone capture
        let input_stream = input_device.build_input_stream(
            &input_config,
            move |samples: &[f32], _| {
                for frame in samples.chunks_exact(2) {
                    input_buffer.force_push([
                        frame[0],
                        frame[1],
                    ]);
                }
            },
            |error| {
                eprintln!("Input stream error: {}", error);
            },
            None,
        )?;

        // Output stream
        let mut primed = false;

        let output_stream = output_device.build_output_stream(
            &output_config,
            move |samples: &mut [f32], _| {
                let gain =
                    output_gain.load(Ordering::Relaxed) as f32
                    / 100.0;

                if !primed
                    && output_buffer.len() >= START_FRAMES
                {
                    primed = true;
                }

                for frame in samples.chunks_exact_mut(2) {
                    if primed {
                        match output_buffer.pop() {
                            Some(stereo) => {
                                frame[0] =
                                    (stereo[0] * gain)
                                    .clamp(-1.0, 1.0);

                                frame[1] =
                                    (stereo[1] * gain)
                                    .clamp(-1.0, 1.0);
                            }
                            None => {
                                frame.fill(0.0);
                                primed = false;
                            }
                        }
                    } else {
                        frame.fill(0.0);
                    }
                }
            },
            |error| {
                eprintln!("Output stream error: {}", error);
            },
            None,
        )?;

        input_stream.play()?;
        output_stream.play()?;

        println!();
        println!("Audio pipeline running.");
        println!("Gain: 100%");
        println!("Sample rate: 48000 Hz");
        println!("Output: VB-CABLE");

        Ok(Self {
            _input_stream: input_stream,
            _output_stream: output_stream,
            gain_percent,
        })
    }

    pub fn set_gain(&self, percent: u32) {
        let percent = percent.clamp(10, 500);

        self.gain_percent.store(
            percent,
            Ordering::Relaxed
        );
    }
}
