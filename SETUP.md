# MicroSwarm host setup

Run these steps on the Ubuntu host before continuing with the repository workflow.

## 1. Restore Docker access

Inspect Docker and its socket:

```bash
sudo systemctl status docker
sudo systemctl restart docker
ls -l /var/run/docker.sock
```

If the socket is owned by `root:docker`, add your user to the Docker group:

```bash
sudo usermod -aG docker "$USER"
```

Log out and back in, then verify access:

```bash
id
docker info
```

If the socket still belongs to `nobody:nogroup`, ask the host administrator to fix the Docker service/socket configuration. Do not make the socket world-writable.

## 2. Restore network access and install Rust

Make sure the host can reach `sh.rustup.rs` and `static.rust-lang.org`. Then run:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source "$HOME/.cargo/env"
rustup update stable
rustup default stable
rustup component add rustfmt clippy
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
```

Verify the installation:

```bash
rustc --version
cargo --version
rustup --version
```

## 3. Install the musl linker

```bash
sudo apt update
sudo apt install musl-tools
musl-gcc --version
```

## 4. Install tokei

```bash
cargo install tokei
tokei --version
```

## 5. Verify the other required tools

```bash
sqlite3 --version
git --version
jq --version
```

These three tools were already available when checked. After completing setup, recheck Docker access and the Rust versions before continuing the MicroSwarm workflow.
