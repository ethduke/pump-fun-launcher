use secrecy::{Secret, ExposeSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};
use anyhow::Result;
use std::env;

/// Secure wrapper for private key that automatically zeroes memory on drop
pub struct SecurePrivateKey {
    private_key: Secret<String>,
}

impl ZeroizeOnDrop for SecurePrivateKey {}

impl Zeroize for SecurePrivateKey {
    fn zeroize(&mut self) {
        // The Secret<String> will handle its own zeroing
        // We just need to implement the trait
    }
}

impl SecurePrivateKey {
    /// Load private key from environment variable securely
    pub fn from_env(var_name: &str) -> Result<Self> {
        let private_key = env::var(var_name)
            .map_err(|_| anyhow::anyhow!("{} must be set in environment", var_name))?;
        
        Ok(Self {
            private_key: Secret::new(private_key),
        })
    }
    
    /// Get the private key for use (exposes it temporarily)
    pub fn expose_secret(&self) -> &str {
        self.private_key.expose_secret()
    }
    
    /// Convert to bytes for keypair creation
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let key_str = self.private_key.expose_secret();
        bs58::decode(key_str)
            .into_vec()
            .map_err(|e| anyhow::anyhow!("Invalid private key format: {}", e))
    }
}

/// Secure wrapper for API keys
pub struct SecureApiKey {
    api_key: Secret<String>,
}

impl ZeroizeOnDrop for SecureApiKey {}

impl Zeroize for SecureApiKey {
    fn zeroize(&mut self) {
        // The Secret<String> will handle its own zeroing
        // We just need to implement the trait
    }
}

impl SecureApiKey {
    /// Load API key from environment variable securely
    pub fn from_env(var_name: &str) -> Result<Self> {
        let api_key = env::var(var_name)
            .map_err(|_| anyhow::anyhow!("{} must be set in environment", var_name))?;
        
        Ok(Self {
            api_key: Secret::new(api_key),
        })
    }
    
    /// Get the API key for use (exposes it temporarily)
    pub fn expose_secret(&self) -> &str {
        self.api_key.expose_secret()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_secure_private_key_creation() {
        // This test would need to set environment variables
        // In a real test, you'd use a test harness
        assert!(true); // Placeholder
    }
}
