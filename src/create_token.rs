use dotenv::dotenv;
use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use solana_commitment_config::CommitmentConfig;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::{env, str::FromStr, sync::Arc};
use log::{info, error};

use crate::vanity_address::{VanityConfig, get_global_vanity_pool};
use crate::secure_credentials::{SecurePrivateKey, SecureApiKey};
pub const IMAGE_FILENAME: &str = "image.png";
pub fn get_default_image_path() -> String {
    format!("data/{}", IMAGE_FILENAME)
}

pub const DEFAULT_NAME_TEMPLATE: &str = "{}";
pub const DEFAULT_DESCRIPTION_TEMPLATE: &str = "{}";
pub const PUMP_FUN_API_URL: &str = "https://pump.fun/api/ipfs";

// Constants from the IDL
const PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const MPL_TOKEN_METADATA_PROGRAM_ID: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
const CREATE_INSTRUCTION_DISCRIMINATOR: &[u8] = &[24, 30, 200, 40, 5, 28, 7, 119];
const GLOBAL_ACCOUNT_SEED: &[u8] = b"global";
const MINT_AUTHORITY_SEED: &[u8] = b"mint-authority";
const BONDING_CURVE_SEED: &[u8] = b"bonding-curve";
const METADATA_SEED: &[u8] = b"metadata";
const EVENT_AUTHORITY_SEED: &[u8] = b"__event_authority";

// Transaction constants
const MIN_REQUIRED_LAMPORTS: u64 = 10_000_000; // 0.01 SOL
const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

pub struct TokenCreator {
    rpc: Arc<RpcClient>,
    program_id: Pubkey,
    payer: Keypair,
}

impl TokenCreator {
    pub fn new() -> Self {
        dotenv().ok(); // Load .env file

        // Load credentials securely
        let secure_private_key = SecurePrivateKey::from_env("PRIVATE_KEY")
            .expect("PRIVATE_KEY must be set in .env");
        let secure_api_key = SecureApiKey::from_env("HELIUS_API_KEY")
            .expect("HELIUS_API_KEY must be set in .env");

        let private_key_bytes = secure_private_key.to_bytes()
            .expect("Invalid private key format");
        let payer = Keypair::try_from(&private_key_bytes[..])
            .expect("Failed to create keypair from private key");
        
        let rpc_url = secure_api_key.expose_secret().to_string();

        let rpc = Arc::new(RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        ));

        let program_id = Pubkey::from_str(PROGRAM_ID).unwrap();

        if VanityConfig::from_env().enabled {
            info!("Vanity address generation enabled (using global pool)");
        } else {
            info!("Vanity address generation disabled");
        }

        TokenCreator {
            rpc,
            program_id,
            payer,
        }
    }

    pub fn get_global_pda(&self) -> Pubkey {
        Pubkey::find_program_address(&[GLOBAL_ACCOUNT_SEED], &self.program_id).0
    }

    pub fn get_bonding_curve_pda(&self, mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[BONDING_CURVE_SEED, mint.as_ref()], &self.program_id).0
    }

    pub fn get_metadata_pda(&self, mint: &Pubkey) -> Pubkey {
        let mpl_program = Pubkey::from_str(MPL_TOKEN_METADATA_PROGRAM_ID).unwrap();
        Pubkey::find_program_address(
            &[METADATA_SEED, mpl_program.as_ref(), mint.as_ref()],
            &mpl_program,
        ).0
    }

    pub fn get_mint_authority_pda(&self) -> Pubkey {
        Pubkey::find_program_address(&[MINT_AUTHORITY_SEED], &self.program_id).0
    }

    pub fn get_event_authority_pda(&self) -> Pubkey {
        Pubkey::find_program_address(&[EVENT_AUTHORITY_SEED], &self.program_id).0
    }
    
    pub async fn get_wallet_balance(&self) -> Result<f64, anyhow::Error> {
        let balance = self.rpc.get_balance(&self.payer.pubkey()).await?;
        Ok(balance as f64 / LAMPORTS_PER_SOL)
    }
    
    pub fn get_wallet_address(&self) -> Pubkey {
        self.payer.pubkey()
    }

    /// Get vanity address pool status (from global pool)
    pub fn get_vanity_status(&self) -> (bool, usize) {
        if let Some(pool) = get_global_vanity_pool() {
            pool.get_vanity_status()
        } else {
            (false, 0)
        }
    }

    /// Get generated vanity address status (from global pool)
    pub fn get_generated_vanity_status(&self) -> (bool, usize, bool) {
        if let Some(pool) = get_global_vanity_pool() {
            (pool.has_generated_addresses(), pool.generated_addresses_count(), pool.is_generation_running())
        } else {
            (false, 0, false)
        }
    }

    /// Check if vanity addresses are enabled (from global pool)
    pub fn is_vanity_enabled(&self) -> bool {
        get_global_vanity_pool().map_or(false, |pool| pool.is_vanity_enabled())
    }


    pub async fn create_token(
        &self,
        name: String,
        symbol: String,
        description: String,
        image_path: Option<String>,
    ) -> Result<(Signature, Pubkey), anyhow::Error> {
        // Check if we're in dry-run mode first
        let dry_run = env::var("DRY_RUN").unwrap_or_else(|_| "false".to_string()).to_lowercase() == "true";
        
        // Try generated vanity first, then fallback to regular
        let (mint_pubkey, mint_keypair, generated_vanity) = if let Some(pool) = get_global_vanity_pool() {
            // Try to get a generated vanity address
            if let Some(generated_addr) = pool.get_generated_vanity_address() {
                info!("Using generated vanity address for token creation: {}", generated_addr.address);
                (generated_addr.address, None, Some(generated_addr))
            } else {
                info!("No generated vanity addresses available, using regular token creation");
                let mint = Keypair::new();
                (mint.pubkey(), Some(mint), None)
            }
        } else {
            info!("Using regular token creation (vanity disabled)");
            let mint = Keypair::new();
            (mint.pubkey(), Some(mint), None)
        };
        
        info!("Creating token...");
        info!("   Name: {}", name);
        info!("   Symbol: {}", symbol);
        info!("   Mint address: {}", mint_pubkey);
        
        // Check wallet balance before proceeding
        let balance = self.rpc.get_balance(&self.payer.pubkey()).await?;
        info!("Wallet balance: {} SOL", balance as f64 / LAMPORTS_PER_SOL);
        
        // Check if we have enough SOL for the transaction
        if balance < MIN_REQUIRED_LAMPORTS {
            return Err(anyhow::anyhow!(
                "Insufficient wallet balance. Current: {} SOL, Required: {} SOL. Please add more SOL to your wallet.",
                balance as f64 / LAMPORTS_PER_SOL,
                MIN_REQUIRED_LAMPORTS as f64 / LAMPORTS_PER_SOL
            ));
        }
        
        // Upload metadata to pump.fun IPFS
        let metadata_uri = self.upload_metadata_to_pumpfun(&name, &symbol, &description, image_path.as_deref()).await?;
        info!("Metadata uploaded to: {}", metadata_uri);
        
        let bonding_curve = self.get_bonding_curve_pda(&mint_pubkey);
        let metadata = self.get_metadata_pda(&mint_pubkey);
        let mint_authority = self.get_mint_authority_pda();
        let global = self.get_global_pda();
        let event_authority = self.get_event_authority_pda();
        
        // Calculate associated token address manually to avoid type mismatch
        let associated_bonding_curve = Pubkey::find_program_address(
            &[
                bonding_curve.as_ref(),
                &Pubkey::new_from_array(spl_token::ID.to_bytes()).to_bytes(),
                mint_pubkey.as_ref(),
            ],
            &Pubkey::new_from_array(spl_associated_token_account::ID.to_bytes()),
        ).0;

        // CREATE instruction discriminator from IDL
        let mut instruction_data = CREATE_INSTRUCTION_DISCRIMINATOR.to_vec();
        
        // Serialize arguments: name, symbol, uri, creator
        let name_bytes = name.as_bytes();
        let symbol_bytes = symbol.as_bytes();
        let uri_bytes = metadata_uri.as_bytes();
        let creator_bytes = self.payer.pubkey().to_bytes();
        
        // Add string length prefixes and data
        instruction_data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(name_bytes);
        instruction_data.extend_from_slice(&(symbol_bytes.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(symbol_bytes);
        instruction_data.extend_from_slice(&(uri_bytes.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(uri_bytes);
        instruction_data.extend_from_slice(&creator_bytes);

        let create_instruction = Instruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta::new(mint_pubkey, true), // mint (always a signer)
                AccountMeta::new_readonly(mint_authority, false),  // mint_authority
                AccountMeta::new(bonding_curve, false),            // bonding_curve
                AccountMeta::new(Pubkey::new_from_array(associated_bonding_curve.to_bytes()), false), // associated_bonding_curve
                AccountMeta::new_readonly(global, false),          // global
                AccountMeta::new_readonly(Pubkey::from_str(MPL_TOKEN_METADATA_PROGRAM_ID).unwrap(), false), // mpl_token_metadata
                AccountMeta::new(metadata, false),                 // metadata
                AccountMeta::new(self.payer.pubkey(), true),       // user (payer)
                AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false), // system_program
                AccountMeta::new_readonly(Pubkey::new_from_array(spl_token::ID.to_bytes()), false),   // token_program
                AccountMeta::new_readonly(Pubkey::new_from_array(spl_associated_token_account::ID.to_bytes()), false), // associated_token_program
                AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false), // rent
                AccountMeta::new_readonly(event_authority, false), // event_authority
                AccountMeta::new_readonly(self.program_id, false), // program
            ],
            data: instruction_data,
        };

        let recent_blockhash = self.rpc.get_latest_blockhash().await?;
        let mut transaction = Transaction::new_with_payer(&[create_instruction], Some(&self.payer.pubkey()));
        
        // Sign the transaction based on address type
        if let Some(generated_vanity) = &generated_vanity {
            // For generated vanity addresses: sign with vanity private key and payer
            info!("Signing transaction with generated vanity address private key");
            transaction.sign(&[generated_vanity.keypair.keypair(), &self.payer], recent_blockhash);
        } else {
            // For regular addresses: sign with payer and mint keypair
            if let Some(mint) = mint_keypair {
                info!("Signing transaction with regular mint keypair");
                transaction.sign(&[&self.payer, &mint], recent_blockhash);
            } else {
                return Err(anyhow::anyhow!("Missing mint keypair for regular address"));
            }
        }

        if dry_run {
            info!("DRY RUN MODE - Not sending transaction");
            info!("   Would create token at address: {}", mint_pubkey);
            info!("   Transaction would be signed and sent to network");
            // Return a fake signature for dry run
            let fake_signature = Signature::default();
            return Ok((fake_signature, mint_pubkey));
        }
        
        info!("Sending transaction...");
        match self.rpc.send_and_confirm_transaction(&transaction).await {
            Ok(signature) => {
                if generated_vanity.is_some() {
                    info!("Generated vanity address used successfully");
                }
                
                info!("Token created successfully!");
                info!("    Transaction signature: {}", signature);
                info!("    Token address: {}", mint_pubkey);
                return Ok((signature, mint_pubkey));
            }
            Err(e) => {
                error!("Token creation failed: {}", e);
                return Err(e.into());
            }
        }
    }

    async fn upload_metadata_to_pumpfun(
        &self,
        name: &str,
        symbol: &str,
        description: &str,
        image_path: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        println!("Uploading metadata to pump.fun IPFS...");
        
        let client = reqwest::Client::new();
        
        // Use provided image path or fall back to image.png
        let actual_image_path = image_path
            .map(String::from)
            .unwrap_or(get_default_image_path());
        println!("Using image file: {}", actual_image_path);
        
        // Read image file
        let image_data = std::fs::read(actual_image_path)?;
        
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(image_data)
                    .file_name(IMAGE_FILENAME)
                    .mime_str("image/png")?,
            )
            .text("name", name.to_string())
            .text("symbol", symbol.to_string())
            .text("description", description.to_string())
            .text("showName", "true")
            .text("createdOn", "https://pump.fun")
            .text("twitter", "")
            .text("telegram", "")
            .text("website", "");

        let response = client
            .post(PUMP_FUN_API_URL)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:126.0) Gecko/20100101 Firefox/126.0")
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Referer", "https://www.pump.fun/create")
            .header("Origin", "https://www.pump.fun")
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to upload metadata: {}", response.status()));
        }

        let result: serde_json::Value = response.json().await?;
        let metadata_uri = result["metadataUri"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No metadataUri in response"))?;

        Ok(metadata_uri.to_string())
    }
}


