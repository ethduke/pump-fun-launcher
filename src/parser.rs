use clap::Parser;
use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

use crate::create_token::{TokenCreator, DEFAULT_NAME_TEMPLATE, DEFAULT_DESCRIPTION_TEMPLATE};
use crate::vanity_address::get_global_vanity_status;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Token symbol (ticker)
    #[arg(short, long)]
    pub symbol: String,

    /// Token name
    #[arg(short, long)]
    pub name: Option<String>,

    /// Token description
    #[arg(short, long)]
    pub description: Option<String>,

    /// Path to token image
    #[arg(short, long)]
    pub image: Option<String>,

    /// Don't wait for vanity address (launch immediately)
    #[arg(long)]
    pub no_vanity: bool,
}

impl Args {
    pub fn get_token_name(&self) -> String {
        if let Some(name) = &self.name {
            name.clone()
        } else {
            DEFAULT_NAME_TEMPLATE.replace("{}", &self.symbol.to_uppercase())
        }
    }

    pub fn get_description(&self) -> String {
        if let Some(description) = &self.description {
            description.clone()
        } else {
            DEFAULT_DESCRIPTION_TEMPLATE.replace("{}", &self.symbol.to_uppercase())
        }
    }

    pub fn get_image_path(&self) -> Option<String> {
        self.image.clone()
    }
}

pub async fn handle_token_creation(args: Args) -> Result<()> {
    log::info!("Processing token creation...");
    
    // Validate symbol length (Metaplex symbol limit is typically 10 characters)
    if args.symbol.len() > 10 {
        return Err(anyhow::anyhow!("Symbol '{}' is too long. Maximum 10 characters allowed.", args.symbol));
    }
    
    let token_name = args.get_token_name();
    let description = args.get_description();
    let image_path = args.get_image_path();
    
    // Validate token name length (Metaplex name limit is typically 32 characters)
    if token_name.len() > 32 {
        return Err(anyhow::anyhow!("Token name '{}' is too long. Maximum 32 characters allowed.", token_name));
    }
    
    log::info!("Creating token with symbol: {}, name: {}, description: {}", 
               args.symbol, token_name, description);
    
    // Check wallet balance first
    let creator = TokenCreator::new();
    let wallet_balance = creator.get_wallet_balance().await?;
    let wallet_address = creator.get_wallet_address();
    
    // Check vanity status
    let (_has_vanity, _pool_size) = creator.get_vanity_status();
    let is_vanity_enabled = creator.is_vanity_enabled();
    
    // Handle vanity address waiting logic
    log::info!("DEBUG: no_vanity={}, is_vanity_enabled={}", args.no_vanity, is_vanity_enabled);
    
    if !args.no_vanity && is_vanity_enabled {
        // Check if we have vanity addresses available in the pool
        let (has_vanity_in_pool, _) = creator.get_vanity_status();
        log::info!("DEBUG: has_vanity_in_pool={}", has_vanity_in_pool);
        
        if !has_vanity_in_pool {
            log::info!("Vanity addresses not ready. Waiting for vanity address generation...");
            log::info!("You can use --no-vanity to launch without waiting for vanity addresses");
            
            // Wait for vanity addresses with status updates every 30 seconds
            loop {
                let (has_generated, generated_count, is_generating) = get_global_vanity_status();
                
                if has_generated {
                    log::info!("Vanity addresses are now ready! Generated: {}", generated_count);
                    break;
                }
                
                if is_generating {
                    log::info!("Still generating vanity addresses... (generated: {}, generating: true)", generated_count);
                } else {
                    log::info!("Vanity address generation not running. Generated: {}", generated_count);
                }
                
                log::info!("Waiting 30 seconds before next check...");
                sleep(Duration::from_secs(30)).await;
            }
        } else {
            log::info!("Vanity addresses are ready! Proceeding with vanity address...");
        }
    } else if args.no_vanity && is_vanity_enabled {
        log::info!("--no-vanity specified. Launching without waiting for vanity addresses...");
    }
    
    // Print initial status with wallet info and vanity status
    let (final_has_vanity, final_pool_size) = creator.get_vanity_status();
    if is_vanity_enabled && final_has_vanity {
        log::info!("Starting deployment with vanity address...");
        log::info!("Wallet: {}", wallet_address);
        log::info!("Balance: {:.4} SOL", wallet_balance);
        log::info!("Vanity addresses ready: {}", final_pool_size);
    } else {
        log::info!("Starting deployment...");
        log::info!("Wallet: {}", wallet_address);
        log::info!("Balance: {:.4} SOL", wallet_balance);
    }
    
    // Create token using TokenCreator
    let (signature, mint_address) = creator.create_token(
        token_name.clone(),
        args.symbol.to_uppercase(), // Symbol is always uppercase
        description.clone(),
        image_path, // Pass the image path (None if no image provided)
    ).await?;
    
    // Print success message with vanity status
    if is_vanity_enabled && final_has_vanity {
        log::info!("{} deployed successfully with vanity address!", args.symbol.to_uppercase());
    } else {
        log::info!("{} deployed successfully!", args.symbol.to_uppercase());
    }
    
    log::info!("Name: {}", token_name);
    log::info!("Symbol: {}", args.symbol.to_uppercase());
    log::info!("Description: {}", description);
    log::info!("Contract: {}", mint_address);
    log::info!("Transaction: {}", signature);
    
    Ok(())
}