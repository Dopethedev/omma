//! Entry point: argument dispatch and the exit-code contract.

use std::process::ExitCode;

use clap::Parser;

use omma::analysis::{self, Options};
use omma::cli::{AnalyzeArgs, Cli, Command, DoctorArgs, McpAction, ServeArgs};
use omma::config::Config;
use omma::error::{Error, Result};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match dispatch(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("omma: {err}");
            err.exit_code()
        }
    };
    ExitCode::from(code as u8)
}

fn dispatch(cli: Cli) -> Result<i32> {
    match cli.command {
        Some(Command::Analyze(args)) => analyze(*args),
        Some(Command::Doctor(args)) => doctor(args),
        Some(Command::Mcp { action }) => match action {
            McpAction::Serve(args) => serve(args),
        },
        None => analyze(cli.analyze),
    }
}

fn analyze(args: AnalyzeArgs) -> Result<i32> {
    let (mut config, used) = Config::load(args.config.as_deref())?;
    omma::init_logging(&config.logging.level);

    if let Some(languages) = args.languages() {
        config.ocr.languages = languages;
    }
    if let Some(max_colors) = args.max_colors {
        if !(1..=64).contains(&max_colors) {
            return Err(Error::Usage(format!(
                "--max-colors must be between 1 and 64, got {max_colors}"
            )));
        }
        config.colors.max_colors = max_colors;
    }
    if let Some(model) = &args.model {
        config.vision.model = model.clone();
    }
    let question = args.question();
    if question.is_some() {
        config.vision.enabled = true;
    }
    // Re-validate after the command line has had its say.
    config.validate(used.as_deref())?;

    let format = args.format(config.general.default_format)?;
    let selection = args.selection(&config)?;
    let image = args.image.clone().ok_or_else(|| {
        Error::Usage("no image given; usage: omma IMAGE | omma analyze IMAGE".to_string())
    })?;

    let options = Options {
        colors: selection.colors,
        ocr: selection.ocr,
        layout: selection.layout,
        ask: question,
        region: args.region.clone(),
        redact: args.redact.clone(),
        allowed_directories: config.mcp.allowed_directories.clone(),
        declared_mime: None,
    };

    let report = analysis::run(&image, &config, &options)?;
    let rendered = omma::output::render(&report, format, args.pretty)?;

    match &args.output {
        Some(path) => {
            std::fs::write(path, rendered.as_bytes()).map_err(|source| Error::Output {
                path: path.clone(),
                source,
            })?
        }
        None => {
            print!("{rendered}");
            if !rendered.ends_with('\n') {
                println!();
            }
        }
    }

    // Backend exit codes: the report is written first, so a caller always has
    // the data; the code says whether what was explicitly asked for happened.
    if options.ask.is_some() && report.vision.is_none() {
        return Ok(3);
    }
    if selection.explicit && selection.ocr && report.ocr.is_none() {
        return Ok(3);
    }
    Ok(0)
}

fn doctor(args: DoctorArgs) -> Result<i32> {
    let (config, used) = Config::load(args.config.as_deref())?;
    omma::init_logging(&config.logging.level);
    print!("{}", omma::doctor::render(&config, used.as_deref()));
    Ok(0)
}

fn serve(args: ServeArgs) -> Result<i32> {
    let (config, _) = Config::load(args.config.as_deref())?;
    omma::init_logging(&config.logging.level);
    omma::mcp::serve(config)?;
    Ok(0)
}
