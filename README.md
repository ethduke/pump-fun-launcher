# Pump.fun Token Launcher

A command-line tool for creating tokens on the Pump Fun platform with vanity address support.

## Setup

Create a `.env` file in the project directory with the following content:

```env
# Solana Private Key (base58 encoded)
PRIVATE_KEY=your_private_key_here

# Helius API Key (required)
HELIUS_API_KEY=your_helius_api_key_here
```

## Usage

### Basic Usage

```bash
# Create token with required symbol
cargo run -- --symbol PVE

# Create token with all parameters
cargo run -- --symbol PVE --name "PVE Token" --description "A great token" --image "path/to/image.png"
```

### Vanity Address Options

```bash
# Wait for vanity addresses (default behavior)
cargo run -- --symbol PVE --name "PVE Token" --description "A great token"

# Launch immediately without waiting for vanity addresses
cargo run -- --symbol PVE --name "PVE Token" --description "A great token" --no-vanity
```

### Command Line Arguments

- `--symbol, -s`: Token symbol (ticker) - **Required**
- `--name, -n`: Token name (optional, defaults to symbol)
- `--description, -d`: Token description (optional, defaults to symbol)
- `--image, -i`: Path to token image (optional, uses data/image.png if not provided)
- `--no-vanity`: Launch immediately without waiting for vanity addresses (default: wait for vanity addresses)

## Features

- **Command-line interface** with clap for easy token creation
- **Vanity address support** with automatic generation and waiting
- **Flexible parameters** - all fields optional except symbol
- **Smart waiting logic** - waits for vanity addresses or launches immediately
- **Status updates** - real-time feedback during vanity address generation
- **Pump.fun integration** - creates tokens using the official IDL structure
- **Environment-based config** - loads private key and API keys from .env file

## Testing

Set `DRY_RUN=true` in your environment to test without creating actual tokens or spending SOL.

## Note

Make sure you have SOL in your wallet for transaction fees before running the tool.
