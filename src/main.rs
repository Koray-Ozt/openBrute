use std::path::PathBuf;
use std::sync::Arc;
use clap::{Parser, ValueEnum};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use openbrute::credentials::{CredentialSource, WordlistMode};
use openbrute::orchestrator::{Orchestrator, OrchestratorConfig};
use openbrute::protocols::http::{HttpTarget, HttpMode as TargetHttpMode, HttpMethod as TargetHttpMethod};
use openbrute::protocols::ssh::SshTarget;
use openbrute::protocols::ftp::FtpTarget;
use openbrute::protocols::smtp::SmtpTarget;
use openbrute::protocols::sql::SqlTarget;
use openbrute::protocols::BruteTarget;

#[derive(Debug, Clone, ValueEnum)]
enum Protocol {
    Http,
    Ssh,
    Ftp,
    Smtp,
    Sql,
}

#[derive(Debug, Clone, ValueEnum)]
enum HttpMode {
    Basic,
    Form,
    Json,
}

#[derive(Debug, Clone, ValueEnum)]
enum HttpMethod {
    Get,
    Post,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "openBrute - Modern, High-Performance Multi-Protocol Brute Force Tool", long_about = None)]
struct Args {
    /// Protocol to target
    #[arg(short, long, value_enum, required_unless_present = "web")]
    protocol: Option<Protocol>,

    /// Target host, IP, URL or connection string (e.g. http://localhost/login)
    #[arg(short, long, required_unless_present = "web")]
    target: Option<String>,

    /// Single username to test
    #[arg(short, long)]
    username: Option<String>,

    /// Path to username wordlist file
    #[arg(short = 'U', long)]
    usernames_file: Option<PathBuf>,

    /// Single password to test
    #[arg(short, long)]
    password: Option<String>,

    /// Path to password wordlist file
    #[arg(short = 'P', long)]
    passwords_file: Option<PathBuf>,

    /// Path to a credential combo file (username:password format per line)
    #[arg(short = 'C', long)]
    combo_file: Option<PathBuf>,

    /// Concurrency level (max worker tasks)
    #[arg(short, long, default_value_t = 10)]
    concurrency: usize,

    /// Optional rate limit in requests per second
    #[arg(short, long)]
    rate_limit: Option<usize>,

    /// Check corresponding lines in username and password files instead of full Cartesian product
    #[arg(long)]
    one_to_one: bool,

    /// Stop execution immediately on first successful credential found
    #[arg(long, default_value_t = true)]
    stop_on_success: bool,

    /// HTTP Auth mode
    #[arg(long, value_enum, default_value = "basic")]
    http_mode: HttpMode,

    /// HTTP Method
    #[arg(long, value_enum, default_value = "post")]
    http_method: HttpMethod,

    /// Form or JSON field name for username
    #[arg(long, default_value = "username")]
    user_field: String,

    /// Form or JSON field name for password
    #[arg(long, default_value = "password")]
    pass_field: String,

    /// Substring indicating successful auth in HTTP response body
    #[arg(long)]
    success_str: Option<String>,

    /// Substring indicating failed auth in HTTP response body
    #[arg(long)]
    fail_str: Option<String>,

    /// Start web dashboard interface
    #[arg(long)]
    web: bool,

    /// Web server port
    #[arg(long, default_value_t = 3000)]
    web_port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let args = Args::parse();

    if args.web {
        let port = args.web_port;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        info!("openBrute Web Dashboard running at http://{}", addr);
        axum::serve(listener, openbrute::web::app()).await?;
        return Ok(());
    }

    let target = args.target.as_ref().ok_or_else(|| anyhow::anyhow!("Target is required when not running in web mode"))?;
    let protocol = args.protocol.as_ref().ok_or_else(|| anyhow::anyhow!("Protocol is required when not running in web mode"))?;

    // Determine credential source
    let source = if let Some(ref combo_path) = args.combo_file {
        CredentialSource::new_combo(combo_path).await?
    } else {
        let usernames = if let Some(ref u) = args.username {
            vec![u.clone()]
        } else if let Some(ref path) = args.usernames_file {
            read_file_lines(path).await?
        } else {
            anyhow::bail!("Must specify either a single username (-u), a usernames wordlist (-U), or a combo file (-C)");
        };

        let passwords = if let Some(ref p) = args.password {
            vec![p.clone()]
        } else if let Some(ref path) = args.passwords_file {
            read_file_lines(path).await?
        } else {
            anyhow::bail!("Must specify either a single password (-p), a passwords wordlist (-P), or a combo file (-C)");
        };

        let mode = if args.one_to_one {
            WordlistMode::OneToOne
        } else {
            WordlistMode::Cartesian
        };

        CredentialSource::from_lists(usernames, passwords, mode)
    };
    info!("Loaded {} total login attempts to try.", source.total_attempts());

    // Select target protocol handler
    let target_handler: Arc<dyn BruteTarget> = match protocol {
        Protocol::Http => {
            let http_method = match args.http_method {
                HttpMethod::Get => TargetHttpMethod::Get,
                HttpMethod::Post => TargetHttpMethod::Post,
            };
            let http_mode = match args.http_mode {
                HttpMode::Basic => TargetHttpMode::Basic,
                HttpMode::Form => TargetHttpMode::Form {
                    user_field: args.user_field.clone(),
                    pass_field: args.pass_field.clone(),
                    success_str: args.success_str.clone(),
                    fail_str: args.fail_str.clone(),
                },
                HttpMode::Json => TargetHttpMode::Json {
                    user_field: args.user_field.clone(),
                    pass_field: args.pass_field.clone(),
                    success_str: args.success_str.clone(),
                    fail_str: args.fail_str.clone(),
                },
            };
            Arc::new(HttpTarget::new(target, http_method, http_mode)?)
        }
        Protocol::Ssh => Arc::new(SshTarget::new(target)?),
        Protocol::Ftp => Arc::new(FtpTarget::new(target)?),
        Protocol::Smtp => Arc::new(SmtpTarget::new(target)?),
        Protocol::Sql => Arc::new(SqlTarget::new(target)?),
    };

    let config = OrchestratorConfig {
        concurrency: args.concurrency,
        rate_limit_per_sec: args.rate_limit,
        stop_on_success: args.stop_on_success,
    };

    let orchestrator = Orchestrator::new(config, target_handler);

    info!("Starting brute force attack against: {}", target);
    let report = orchestrator.run(source).await?;
    info!("Attack finished.");

    info!("Report Summary:");
    info!("  Total attempts made: {}", report.total_attempts);
    info!("  Successes found:     {}", report.successes.len());
    info!("  Failures:            {}", report.failures);
    info!("  Blocked/Throttled:   {}", report.blocked);

    for success in &report.successes {
        println!("FOUND -> Username: {}, Password: {}", success.username, success.password);
    }

    Ok(())
}

async fn read_file_lines(path: &std::path::Path) -> Result<Vec<String>, std::io::Error> {
    use tokio::fs::File;
    use tokio::io::{AsyncBufReadExt, BufReader};
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut result = Vec::new();
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            result.push(trimmed.to_string());
        }
    }
    Ok(result)
}
