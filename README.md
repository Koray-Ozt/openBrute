# openBrute 🚀

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Asynchronous](https://img.shields.io/badge/Async-Tokio-green.svg)](https://tokio.rs/)

`openBrute` is a modern, high-performance, asynchronous multi-protocol brute-force tool written in Rust. It leverages the power of **Tokio**'s event loop to achieve extreme concurrency while keeping a low memory footprint.

Developed with a modular architecture, every target protocol engine is isolated and implements a unified interface, ensuring extensibility and safety.

---

## 🌟 Key Features

- **Asynchronous Execution**: Bounded concurrency managed via semaphores and channels. No heavy operating system threads.
- **Rate Limiting**: Built-in request throttle control (requests per second) to prevent IP bans or server crashes.
- **Stop on Success**: Option to abort authentication checks immediately after finding the first valid credential pair.
- **Dual Wordlist Modes**: 
  - `Cartesian`: Tests all passwords against every username (NxM combinations).
  - `One-to-One`: Corresponds line-by-line between usernames and passwords files.
- **Modular & Lightweight**: Designed strictly under a 600-line limit per file for peak code legibility and maintainability.

---

## 🔌 Supported Protocols

| Protocol | Engine Backend | Authentication Modes |
|:---|:---|:---|
| **HTTP/HTTPS** | `reqwest` & `rustls` | Basic Auth, HTML Form GET/POST, JSON API POST |
| **SSH** | `russh` | Password authentication |
| **FTP** | `suppaftp` | Standard authentication |
| **SMTP** | `lettre` | Connection test (`NOOP` based check after authentication) |
| **SQL Databases** | `sqlx` | MySQL & PostgreSQL connection auth tests |

---

## 🛠️ Installation

Ensure you have Rust and Cargo installed:

```bash
# Clone the repository
git clone https://github.com/<your-username>/openBrute.git
cd openBrute

# Build the release binary
cargo build --release

# The compiled binary will be located at:
# ./target/release/openbrute
```

---

## 🚀 Usage Guide

```text
openBrute - Modern, High-Performance Multi-Protocol Brute Force Tool

Usage: openbrute [OPTIONS] --protocol <PROTOCOL> --target <TARGET>

Options:
  -p, --protocol <PROTOCOL>        Protocol to target [possible values: http, ssh, ftp, smtp, sql]
  -t, --target <TARGET>            Target host, IP, URL or connection string
  -u, --username <USERNAME>        Single username to test
  -U, --usernames-file <FILE>      Path to username wordlist file
  -s, --password <PASSWORD>        Single password to test
  -P, --passwords-file <FILE>      Path to password wordlist file
  -c, --concurrency <CONCURRENCY>  Concurrency level (max worker tasks) [default: 10]
  -r, --rate-limit <RATE_LIMIT>    Optional rate limit in requests per second
      --one-to-one                 Check corresponding lines instead of full Cartesian product
      --stop-on-success            Stop execution immediately on first success [default: true]
      --http-mode <HTTP_MODE>      HTTP Auth mode [default: basic] [possible values: basic, form, json]
      --http-method <HTTP_METHOD>  HTTP Method [default: post] [possible values: get, post]
      --user-field <USER_FIELD>    Form or JSON field name for username [default: username]
      --pass-field <PASS_FIELD>    Form or JSON field name for password [default: password]
      --success-str <SUCCESS_STR>  Substring indicating successful auth in HTTP response
      --fail-str <FAIL_STR>        Substring indicating failed auth in HTTP response
  -h, --help                       Print help
  -V, --version                    Print version
```

### Examples

#### 1. HTTP JSON API Login Brute Force
```bash
./target/release/openbrute \
  --protocol http \
  --target "https://api.example.com/v1/auth/login" \
  -U usernames.txt \
  -P passwords.txt \
  --http-mode json \
  --user-field "email" \
  --pass-field "passwd" \
  --success-str '"token":'
```

#### 2. SSH Password Brute Force
```bash
./target/release/openbrute \
  --protocol ssh \
  --target "192.168.1.50:22" \
  -u admin \
  -P top1000_passwords.txt \
  --concurrency 15
```

#### 3. PostgreSQL Database Brute Force
```bash
./target/release/openbrute \
  --protocol sql \
  --target "postgres://127.0.0.1:5432/postgres" \
  -U pg_users.txt \
  -P pg_passwords.txt \
  --concurrency 5
```

#### 4. FTP Brute Force with Rate Limiting
```bash
./target/release/openbrute \
  --protocol ftp \
  --target "ftp.example.org:21" \
  -U users.txt \
  -P passwords.txt \
  --rate-limit 2
```

---

## 🏛️ License

This project is licensed under the Apache License 2.0. See the [LICENSE](LICENSE) file for details.
