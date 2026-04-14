# FakeRoute

[![Crates.io](https://img.shields.io/crates/v/fakeroute)](https://crates.io/crates/fakeroute)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> Instant JSON API mock server for frontend development.

FakeRoute watches a directory of `.json` files and serves them as REST endpoints.  
It's designed to be **zero‑configuration**, **fast**, and **beautiful** in the terminal.

[![FakeRoute Demo](https://asciinema.org/a/HMwgMeFLrGZXMvXE.svg)](https://asciinema.org/a/HMwgMeFLrGZXMvXE)

## Features

- 🚀 **Zero config** – just run it inside a folder with JSON files
- 📁 **Nested routes** – folder structure becomes the API path
- 🔄 **Hot reload** – files are watched and changes are applied instantly
- 🌐 **CORS enabled** – works out of the box with browser applications
- 🔒 **Path traversal protection** – safe by default
- 🎨 **Pretty logging** – coloured output with request status and timing
- 🔌 **Port auto‑selection** – if the default port is busy, it finds the next available one

## Installation

### Using Cargo

```bash
cargo install fakeroute
```

### From source

```bash
git clone https://github.com/NameOfShadow/fakeroute
cd fakeroute
cargo build --release
```

The binary will be located at `target/release/fakeroute`.

## Usage

```bash
fakeroute [OPTIONS]
```

### Options

| Option            | Description                          | Default |
|-------------------|--------------------------------------|---------|
| `-p, --port`      | Port to listen on                    | `3000`  |
| `-d, --dir`       | Directory containing mock JSON files | `mocks` |
| `-h, --help`      | Show help message                    |         |

### Examples

```bash
# Start with default settings (port 3000, ./mocks folder)
fakeroute

# Use a custom port and folder
fakeroute -p 8080 -d ./api-mocks

# If port 3000 is busy, FakeRoute will automatically try 3001, 3002, ...
```

## How it works

1. Place your JSON files inside the mock directory (default: `./mocks`).
2. The file structure determines the API endpoints:
   - `mocks/users.json` → `GET /users`
   - `mocks/posts/1.json` → `GET /posts/1`
   - `mocks/products/featured.json` → `GET /products/featured`
3. Run `fakeroute` – it will display all detected endpoints.
4. Make requests to `http://localhost:3000/your-path`.

### Example

Given the following folder structure:

```
mocks/
├── users.json
├── posts/
│   ├── 1.json
│   └── 2.json
└── products/
    └── featured.json
```

FakeRoute will serve:

```
GET /users              → contents of users.json
GET /posts/1            → contents of posts/1.json
GET /posts/2            → contents of posts/2.json
GET /products/featured  → contents of products/featured.json
```

### Response Headers

All responses include:

- `Content-Type: application/json`
- `Access-Control-Allow-Origin: *`

## Development

### Prerequisites

- Rust 1.70 or later

### Build & Run

```bash
cargo run -- -d ./examples
```

## License

This project is licensed under the MIT License – see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

---

Made with ❤️ for frontend developers who just want to prototype.