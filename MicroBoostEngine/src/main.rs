
mod audio;

use std::error::Error;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn Error>> {
    println!("=================================");
    println!("      MICROBOOST ENGINE");
    println!("=================================");

    let pipeline = audio::AudioPipeline::start()?;

    println!();
    println!("Microphone boost control");
    println!("Enter a value between 10 and 500.");
    println!("Enter 'q' to stop the engine.");
    println!();

    loop {
        print!("Boost % > ");
        io::stdout().flush()?;

        let mut input = String::new();

        if io::stdin().read_line(&mut input)? == 0 {
            break;
        }

        let command = input.trim();

        if command.eq_ignore_ascii_case("q")
            || command.eq_ignore_ascii_case("quit")
        {
            break;
        }

        match command.parse::<u32>() {
            Ok(percent) if (10..=500).contains(&percent) => {
                pipeline.set_gain(percent);
                println!("Boost set to {}%", percent);
            }
            _ => {
                println!(
                    "Invalid value. Enter 10-500 or 'q'."
                );
            }
        }
    }

    drop(pipeline);

    println!("Audio engine stopped.");

    Ok(())
}
