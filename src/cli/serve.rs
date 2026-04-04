use clap::Args;

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Host to bind to
    #[arg(short, long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to listen on
    #[arg(short, long, default_value = "8686")]
    pub port: u16,
}
