use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{ArgAction, Args, Parser, Subcommand};
use console::style;
use gliner::types::{
    HealthResponse, InfoResponse, PIIEntity, ScanPathRequest, ScanResponse, ScanStatus,
};
use reqwest::Url;

#[derive(Parser, Debug)]
#[command(
    name = "gliner-client",
    about = "CLI client for the GLiNER PII detection server",
    version
)]
struct ClientArgs {
    #[arg(
        long,
        global = true,
        default_value_t = default_server(),
        help = "GLiNER server URL"
    )]
    server: String,

    #[command(subcommand)]
    command: ClientCommand,
}

#[derive(Subcommand, Debug)]
enum ClientCommand {
    /// Check server health status.
    Health,
    /// Fetch server configuration and model information.
    Info,
    /// Scan a path accessible to the server.
    ScanPath(ScanPathArgs),
}

#[derive(Args, Debug)]
struct ScanPathArgs {
    /// Path to scan (must be accessible to the server)
    path: PathBuf,

    /// Disable recursive directory scanning
    #[arg(long, action = ArgAction::SetTrue)]
    no_recursive: bool,

    /// Custom labels to scan for (comma-separated)
    #[arg(long, value_delimiter = ',')]
    labels: Option<Vec<String>>,

    /// Named label profile to use
    #[arg(long)]
    label_profile: Option<String>,

    /// Include extended PII labels
    #[arg(long, action = ArgAction::SetTrue)]
    extended_labels: bool,

    /// Include private conversation detection labels
    #[arg(long, action = ArgAction::SetTrue)]
    private_conversation: bool,

    /// Include log data detection labels
    #[arg(long, action = ArgAction::SetTrue)]
    log_detection: bool,

    /// Override confidence threshold (0.0-1.0)
    #[arg(long)]
    threshold: Option<f32>,

    /// Override maximum chunk size in characters
    #[arg(long)]
    chunk_size: Option<usize>,

    /// Override batch size (chunks processed in parallel)
    #[arg(long)]
    batch_size: Option<usize>,

    /// Override maximum workers for directory scanning
    #[arg(long)]
    max_workers: Option<usize>,

    /// Override maximum file size (MB)
    #[arg(long)]
    max_file_size: Option<f32>,

    /// Disable false positive filtering
    #[arg(long, action = ArgAction::SetTrue)]
    disable_filtering: bool,

    /// Save results to a JSON file
    #[arg(long)]
    output: Option<PathBuf>,

    /// Display all files including those without detected PII
    #[arg(long, action = ArgAction::SetTrue)]
    show_all: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = ClientArgs::parse();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("Failed to create HTTP client")?;

    match args.command {
        ClientCommand::Health => {
            let response = fetch_health(&client, &args.server).await?;
            display_health(&response);
        }
        ClientCommand::Info => {
            let response = fetch_info(&client, &args.server).await?;
            display_info(&response);
        }
        ClientCommand::ScanPath(scan_args) => {
            let response = scan_path(&client, &args.server, &scan_args).await?;
            if let Some(path) = &scan_args.output {
                std::fs::write(
                    path,
                    serde_json::to_string_pretty(&response)
                        .context("Failed to serialise scan response")?,
                )
                .with_context(|| format!("Failed to write results to {}", path.display()))?;
                println!(
                    "{}",
                    style(format!("Results saved to {}", path.display())).green()
                );
            }
            display_scan_results(&response, scan_args.show_all);
        }
    }

    Ok(())
}

async fn fetch_health(client: &reqwest::Client, server: &str) -> Result<HealthResponse> {
    let url = Url::parse(server)
        .context("Invalid server URL")?
        .join("api/v1/health")
        .context("Failed to build health endpoint URL")?;

    client
        .get(url)
        .send()
        .await
        .context("Health request failed")?
        .error_for_status()
        .context("Health request returned error status")?
        .json::<HealthResponse>()
        .await
        .context("Failed to parse health response")
}

async fn fetch_info(client: &reqwest::Client, server: &str) -> Result<InfoResponse> {
    let url = Url::parse(server)
        .context("Invalid server URL")?
        .join("api/v1/info")
        .context("Failed to build info endpoint URL")?;

    client
        .get(url)
        .send()
        .await
        .context("Info request failed")?
        .error_for_status()
        .context("Info request returned error status")?
        .json::<InfoResponse>()
        .await
        .context("Failed to parse info response")
}

async fn scan_path(
    client: &reqwest::Client,
    server: &str,
    args: &ScanPathArgs,
) -> Result<ScanResponse> {
    let url = Url::parse(server)
        .context("Invalid server URL")?
        .join("api/v1/scan/path")
        .context("Failed to build scan endpoint URL")?;

    let request = ScanPathRequest {
        path: args.path.to_string_lossy().to_string(),
        recursive: !args.no_recursive,
        labels: args.labels.clone(),
        label_profile: args.label_profile.clone(),
        use_extended_labels: args.extended_labels,
        use_private_conversation: args.private_conversation,
        use_log_signals: args.log_detection,
        threshold: args.threshold,
        chunk_size: args.chunk_size,
        batch_size: args.batch_size,
        max_workers: args.max_workers,
        max_file_size: args.max_file_size,
        filter_false_positives: Some(!args.disable_filtering),
    };

    client
        .post(url)
        .json(&request)
        .send()
        .await
        .context("Scan request failed")?
        .error_for_status()
        .context("Scan request returned error status")?
        .json::<ScanResponse>()
        .await
        .context("Failed to parse scan response")
}

fn display_health(response: &HealthResponse) {
    println!("{}", style("GLiNER Server Health").bold().cyan());
    println!("Status     : {}", style(&response.status).green());
    println!("Model      : {}", response.model_name);
    println!("Device     : {}", response.device);
    println!(
        "GPU Enabled: {}",
        if response.gpu_enabled { "Yes" } else { "No" }
    );
    println!("Labels     : {}", response.labels_count);
}

fn display_info(response: &InfoResponse) {
    println!("{}", style("GLiNER Server Information").bold().cyan());
    println!("Model      : {}", response.model_name);
    println!("Device     : {}", response.device);
    println!(
        "GPU Enabled: {}",
        if response.gpu_enabled { "Yes" } else { "No" }
    );
    println!("Default Labels : {}", response.default_labels.len());
    println!("Extended Labels: {}", response.extended_labels.len());
    println!(
        "Conversation Labels: {}",
        response.private_conversation_labels.len()
    );
    println!("Log Labels       : {}", response.log_labels.len());
    println!();
    println!("{}", style("Default Configuration").bold());
    println!("  Threshold    : {:.2}", response.default_config.threshold);
    println!("  Chunk Size   : {}", response.default_config.chunk_size);
    println!("  Batch Size   : {}", response.default_config.batch_size);
    println!(
        "  Max File Size: {:.1} MB",
        response.default_config.max_file_size_mb
    );
}

fn display_scan_results(response: &ScanResponse, show_all: bool) {
    println!();
    println!("{}", style("Scan Summary").bold().cyan());
    println!("Total Files    : {}", response.summary.total_files);
    println!("Files with PII : {}", response.summary.files_with_pii);
    println!("Total PII Count: {}", response.summary.total_pii);
    println!("Errors         : {}", response.summary.errors);
    println!();

    for result in &response.results {
        match result.status {
            ScanStatus::Error => {
                println!(
                    "{} {}",
                    style("✗").red(),
                    result.file.as_deref().unwrap_or("<unknown file>")
                );
                if let Some(error) = &result.error {
                    println!("    {}", style(error).red());
                }
                println!();
            }
            ScanStatus::Skipped => {
                if show_all {
                    println!(
                        "{} {}",
                        style("△").yellow(),
                        result.file.as_deref().unwrap_or("<unknown file>")
                    );
                    if let Some(reason) = &result.error {
                        println!("    {}", style(reason).yellow());
                    }
                    println!();
                }
            }
            ScanStatus::Success => {
                if !show_all && result.pii_count == 0 {
                    continue;
                }

                let header_style = if result.pii_count > 0 {
                    style("🔍").red()
                } else {
                    style("✓").green()
                };
                let file_name = result.file.as_deref().unwrap_or("<unknown file>");
                let summary = if result.pii_count > 0 {
                    format!("{} ({} entities)", file_name, result.pii_count)
                } else {
                    format!("{} (no PII detected)", file_name)
                };
                println!("{} {}", header_style, style(summary).bold());

                if result.pii_count == 0 {
                    println!();
                    continue;
                }

                let by_label = group_by_label(&result.entities);
                for (label, entities) in by_label {
                    println!(
                        "  {} {} ({})",
                        style("•").yellow(),
                        style(label).bold().yellow(),
                        entities.len()
                    );
                    for entity in entities.iter().take(10) {
                        let confidence = entity.score;
                        let colour = if confidence > 0.7 {
                            style(format!("{:.2}", confidence)).red()
                        } else if confidence > 0.5 {
                            style(format!("{:.2}", confidence)).yellow()
                        } else {
                            style(format!("{:.2}", confidence)).white()
                        };
                        println!("      \"{}\" (confidence {})", entity.text, colour);
                    }
                    if entities.len() > 10 {
                        println!("      {} more...", entities.len() - 10);
                    }
                }
                println!();
            }
        }
    }
}

fn group_by_label(entities: &[PIIEntity]) -> BTreeMap<&str, Vec<&PIIEntity>> {
    let mut map: BTreeMap<&str, Vec<&PIIEntity>> = BTreeMap::new();
    for entity in entities {
        map.entry(&entity.label).or_default().push(entity);
    }
    map
}

fn default_server() -> String {
    env::var("GLINER_SERVER_URL").unwrap_or_else(|_| "http://localhost:8000".to_string())
}
