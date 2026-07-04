use clap::Parser;

/// Anthropic <-> Kiro API client
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// configfilepath
    #[arg(short, long)]
    pub config: Option<String>,

    /// credential filepath
    #[arg(long)]
    pub credentials: Option<String>,
}
