use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}, OnceLock};
use std::collections::VecDeque;
use std::thread;
use std::time::{Duration, Instant};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
};
use anyhow::Result;
use log::{info, error};
use rayon::prelude::*;
use zeroize::{Zeroize, ZeroizeOnDrop};

// Constants
pub const TARGET_VANITY_COUNT: usize = 10;
pub const VANITY_SUFFIX: &str = "pump";

#[derive(Debug, Clone)]
pub struct VanityAddress {
    pub seed: String,
    pub address: Pubkey,
}

/// Result of a successful vanity address search.
#[derive(Debug)]
pub struct VanityResult {
    pub keypair: Keypair,
    pub elapsed: Duration,
    pub attempts: u64,
}

/// Secure wrapper for keypair that automatically zeroes memory on drop
pub struct SecureKeypair {
    keypair: Keypair,
}

impl ZeroizeOnDrop for SecureKeypair {}

impl Zeroize for SecureKeypair {
    fn zeroize(&mut self) {
        // Note: Solana's Keypair doesn't expose mutable access to secret bytes
        // The best we can do is ensure the keypair is dropped
        // The Secret<String> in our secure credentials will handle zeroing
        // This is a limitation of the Solana SDK
    }
}

impl SecureKeypair {
    pub fn new(keypair: Keypair) -> Self {
        Self { keypair }
    }
    
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }
    
    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }
    
    pub fn sign(&self, message: &[u8]) -> solana_sdk::signature::Signature {
        self.keypair.sign_message(message)
    }
}

/// Generated vanity address with secure private key storage
pub struct GeneratedVanityAddress {
    pub keypair: SecureKeypair,
    pub seed: String,
    pub address: Pubkey,
    // Removed private_key_base64 - no longer storing private key in multiple formats
}

pub struct VanityAddressPool {
    generated_addresses: Arc<Mutex<VecDeque<GeneratedVanityAddress>>>,
    is_generating: Arc<AtomicBool>,
    generation_thread: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl VanityAddressPool {
    pub fn new() -> Self {
        info!("Creating new VanityAddressPool");
        Self {
            generated_addresses: Arc::new(Mutex::new(VecDeque::new())),
            is_generating: Arc::new(AtomicBool::new(false)),
            generation_thread: Arc::new(Mutex::new(None)),
        }
    }

    /// Get vanity address pool status (now only for generated addresses)
    pub fn get_vanity_status(&self) -> (bool, usize) {
        (self.has_generated_addresses(), self.generated_addresses_count())
    }

    /// Check if vanity addresses are enabled (configuration-based)
    pub fn is_vanity_enabled(&self) -> bool {
        VanityConfig::from_env().enabled
    }

    /// Start background generation of vanity addresses
    pub fn start_background_generation(&self) -> Result<()> {
        if self.is_generating.load(Ordering::SeqCst) {
            info!("Background vanity generation already running");
            return Ok(());
        }

        let generated_addresses = Arc::clone(&self.generated_addresses);
        let is_generating = Arc::clone(&self.is_generating);
        let generation_thread = Arc::clone(&self.generation_thread);

        is_generating.store(true, Ordering::SeqCst);

        let handle = thread::spawn(move || {
            info!("Starting background vanity address generation for suffix: '{}'", VANITY_SUFFIX);
            info!("Target count: {} addresses", TARGET_VANITY_COUNT);
            
            // Initialize rayon thread pool
            let num_threads = num_cpus::get();
            info!("Using {} CPU threads for parallel generation", num_threads);
            rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build_global()
                .ok();

            let mut total_attempts = 0u64;
            let start_time = Instant::now();

            let mut last_status_time = Instant::now();
            let status_interval = Duration::from_secs(30); // Log status every 30 seconds

            while is_generating.load(Ordering::SeqCst) {
                let current_count = {
                    let pool = generated_addresses.lock().unwrap();
                    pool.len()
                };

                if current_count >= TARGET_VANITY_COUNT {
                    info!("Target vanity address count reached ({}), stopping generation", TARGET_VANITY_COUNT);
                    break;
                }

                // Log status every 30 seconds
                if last_status_time.elapsed() >= status_interval {
                    info!("Vanity generation status: {} addresses generated, {} remaining, {} total attempts", 
                          current_count, TARGET_VANITY_COUNT - current_count, total_attempts);
                    last_status_time = Instant::now();
                }

                info!("🔍 Generating vanity address #{} (current pool: {})", current_count + 1, current_count);
                
                // Generate one vanity address
                if let Ok(result) = Self::find_vanity_address_with_suffix(VANITY_SUFFIX, num_threads) {
                    total_attempts += result.attempts;
                    let pubkey_str = result.keypair.pubkey().to_string();
                    
                    // Create secure keypair wrapper
                    let secure_keypair = SecureKeypair::new(result.keypair);
                    let address = secure_keypair.pubkey();
                    
                    let generated_addr = GeneratedVanityAddress {
                        keypair: secure_keypair,
                        seed: format!("vanity_{}", current_count),
                        address,
                    };

                    {
                        let mut pool = generated_addresses.lock().unwrap();
                        pool.push_back(generated_addr);
                        info!("Generated vanity address #{}: {}", current_count + 1, pubkey_str);
                        info!("    Attempts: {}, Time: {:?}, Total attempts so far: {}", 
                              result.attempts, result.elapsed, total_attempts);
                        // Removed private key logging for security
                    }
                } else {
                    error!("Failed to generate vanity address");
                }
            }

            let total_time = start_time.elapsed();
            info!("Background vanity address generation completed");
            info!("    Total time: {:?}", total_time);
            info!("    Total attempts: {}", total_attempts);
            info!("    Rate: {:.2} attempts/second", total_attempts as f64 / total_time.as_secs_f64());

            is_generating.store(false, Ordering::SeqCst);
        });

        {
            let mut thread_guard = generation_thread.lock().unwrap();
            *thread_guard = Some(handle);
        }

        info!("Background vanity address generation started successfully");
        Ok(())
    }

    /// Stop background generation of vanity addresses
    pub fn stop_background_generation(&self) {
        if !self.is_generating.load(Ordering::SeqCst) {
            info!("Background vanity generation not running");
            return;
        }

        self.is_generating.store(false, Ordering::SeqCst);
        
        // Wait for thread to finish
        if let Some(handle) = self.generation_thread.lock().unwrap().take() {
            let _ = handle.join();
        }

        info!("Background vanity address generation stopped");
    }

    /// Get a generated vanity address for token creation
    pub fn get_generated_vanity_address(&self) -> Option<GeneratedVanityAddress> {
        let mut pool = self.generated_addresses.lock().unwrap();
        let remaining_count = pool.len();
        let result = pool.pop_front();
        
        if let Some(ref addr) = result {
            info!("Using generated vanity address: {}", addr.address);
            info!("Remaining addresses in pool: {}", remaining_count - 1);
        } else {
            info!("No generated vanity addresses available in pool");
        }
        
        result
    }

    /// Check if we have generated vanity addresses available
    pub fn has_generated_addresses(&self) -> bool {
        let pool = self.generated_addresses.lock().unwrap();
        !pool.is_empty()
    }

    /// Get count of generated vanity addresses
    pub fn generated_addresses_count(&self) -> usize {
        let pool = self.generated_addresses.lock().unwrap();
        pool.len()
    }

    /// Check if background generation is running
    pub fn is_generation_running(&self) -> bool {
        self.is_generating.load(Ordering::SeqCst)
    }

    /// Searches for a Solana keypair whose public key ends with the given suffix.
    fn find_vanity_address_with_suffix(suffix: &str, num_threads: usize) -> Result<VanityResult> {
        let found = AtomicBool::new(false);
        let attempts = AtomicU64::new(0);
        let start_time = Instant::now();
        let result = Arc::new(Mutex::new(None::<Keypair>));
        let mut last_progress_time = Instant::now();
        let progress_interval = Duration::from_secs(30);

        rayon::ThreadPoolBuilder::new().num_threads(num_threads).build_global().ok();

        while !found.load(Ordering::SeqCst) {
            let result_clone = Arc::clone(&result);
            (0..100_000).into_par_iter().for_each(|_| {
                if found.load(Ordering::SeqCst) {
                    return;
                }
                let keypair = Keypair::new();
                let pubkey_str = keypair.pubkey().to_string();
                attempts.fetch_add(1, Ordering::Relaxed);
                if pubkey_str.ends_with(suffix) {
                    found.store(true, Ordering::SeqCst);
                    let mut result_guard = result_clone.lock().unwrap();
                    *result_guard = Some(keypair);
                }
            });
            
            // Log progress every 30 seconds during the search
            if last_progress_time.elapsed() >= progress_interval {
                let current_attempts = attempts.load(Ordering::Relaxed);
                let elapsed = start_time.elapsed();
                let rate = current_attempts as f64 / elapsed.as_secs_f64();
                info!("🔍 Still searching for '{}' suffix... {} attempts in {:?} ({:.0} attempts/sec)", 
                      suffix, current_attempts, elapsed, rate);
                last_progress_time = Instant::now();
            }
        }

        let keypair = result.lock().unwrap().take().expect("Keypair should be found");
        Ok(VanityResult {
            keypair,
            elapsed: start_time.elapsed(),
            attempts: attempts.load(Ordering::Relaxed),
        })
    }

}

#[derive(Debug, Clone)]
pub struct VanityConfig {
    pub enabled: bool,
}

impl VanityConfig {
    pub fn from_env() -> Self {
        let env_value = std::env::var("VANITY_ENABLED")
            .unwrap_or_else(|_| "true".to_string());
        let enabled = env_value.to_lowercase() == "true";
        
        println!("DEBUG: Vanity configuration loaded");

        Self { enabled }
    }
}

// Global vanity address pool singleton
static GLOBAL_VANITY_POOL: OnceLock<Arc<VanityAddressPool>> = OnceLock::new();

/// Initialize the global vanity address pool
pub fn init_global_vanity_pool() -> Result<()> {
    if GLOBAL_VANITY_POOL.get().is_some() {
        info!("🔄 Global vanity pool already initialized");
        return Ok(());
    }

    info!("🚀 Initializing global vanity address pool");
    let pool = Arc::new(VanityAddressPool::new());
    
    // Start background generation immediately
    if let Err(e) = pool.start_background_generation() {
        error!("❌ Failed to start global vanity generation: {}", e);
        return Err(e);
    }
    
    GLOBAL_VANITY_POOL.set(pool)
        .map_err(|_| anyhow::anyhow!("Failed to set global vanity pool"))?;
    
    info!("✅ Global vanity address pool initialized and generation started");
    Ok(())
}

/// Get the global vanity address pool
pub fn get_global_vanity_pool() -> Option<Arc<VanityAddressPool>> {
    GLOBAL_VANITY_POOL.get().cloned()
}

/// Get vanity address pool status from global pool
pub fn get_global_vanity_status() -> (bool, usize, bool) {
    if let Some(pool) = get_global_vanity_pool() {
        (pool.has_generated_addresses(), pool.generated_addresses_count(), pool.is_generation_running())
    } else {
        (false, 0, false)
    }
}
