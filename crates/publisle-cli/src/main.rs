#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
use clap::{Args, Parser, Subcommand};
use publisle_pdf::model::{FontSpec, Input};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    name = "publisle-rs",
    version,
    about = "Native Publisle PDF and bounded TeX math tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Compile resolved native input JSON. Markdown belongs to the npm CLI.
    Pdf {
        input: PathBuf,
        output: PathBuf,
        #[command(flatten)]
        fonts: FontArgs,
    },
    /// Parse bounded TeX math and print MathML.
    Math {
        tex: String,
        #[arg(long)]
        mathml: bool,
    },
    /// Validate the files in a pinned font manifest.
    Fonts {
        #[command(subcommand)]
        command: FontCommand,
    },
}
#[derive(Subcommand)]
enum FontCommand {
    Check {
        #[command(flatten)]
        fonts: FontArgs,
    },
}
#[derive(Args)]
struct FontArgs {
    #[arg(
        long,
        env = "PUBLISLE_NATIVE_FONTS",
        help = "JSON array of path, role and optional index entries"
    )]
    fonts: PathBuf,
}
fn bounded(path: &PathBuf, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("Input exceeds size limit".into());
    }
    Ok(bytes)
}
fn load_fonts(args: &FontArgs) -> Result<Vec<FontSpec>, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&bounded(&args.fonts, 65536)?)?)
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Pdf {
            input,
            output,
            fonts,
        } => {
            let mut input: Input = serde_json::from_slice(&bounded(&input, 4 * 1024 * 1024)?)?;
            input.fonts = load_fonts(&fonts)?;
            let (pdf, report) = publisle_pdf::compile(&input)?;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?
                .write_all(&pdf)?;
            println!("{}", serde_json::to_string(&report)?);
        }
        Command::Math { tex, mathml: _ } => {
            let (tree, warnings) = publisle_math::parse_with_diagnostics(&tex)?;
            for warning in warnings {
                eprintln!("{warning}");
            }
            println!(
                "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{}</math>",
                tree.mathml()
            );
        }
        Command::Fonts {
            command: FontCommand::Check { fonts },
        } => {
            let fonts = load_fonts(&fonts)?;
            publisle_pdf::check_fonts(&fonts)?;
            println!("Validated {} font faces.", fonts.len());
        }
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
