use log::{info, error};
use anyhow::Result;
use clap::Parser;

use pumpfun_launcher::parser::{Args, handle_token_creation};
use pumpfun_launcher::vanity_address::{init_global_vanity_pool, get_global_vanity_status};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    info!("Starting Pump.fun Token Launcher...");
    
    dotenv::dotenv().ok();
    
    // Initialize global vanity address pool first
    info!("Initializing global vanity address generation...");
    if let Err(e) = init_global_vanity_pool() {
        error!("Failed to initialize global vanity pool: {}", e);
    }
    
    // Check vanity address status
    let (has_generated, generated_count, is_generating) = get_global_vanity_status();
    
    info!("Global vanity address status - Generated: {} (count: {}), Generating: {}", 
          has_generated, generated_count, is_generating);
    
    // Parse command line arguments
    let args = Args::parse();
    
    // Handle token creation
    if let Err(e) = handle_token_creation(args).await {
        error!("Failed to create token: {}", e);
        std::process::exit(1);
    }
    
    Ok(())
}