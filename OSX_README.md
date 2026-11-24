These are instructions for building **Muabase** on macOS (Apple Silicon / M-series CPUs).

1. Install the following Homebrew dependencies:

```bash
brew install binaryen yarn node wasm-pack
```

2. Install Rust from <https://www.rust-lang.org/tools/install>:

   - If you installed Rust via Homebrew, uninstall it first.
   - Ensure `~/.cargo/bin` is on your `PATH`.
   - Install WebAssembly targets for Rust:

```bash
rustup target add wasm32-unknown-unknown wasm32-wasip1
```

3. Install WASI SDK 29.0 from <https://github.com/WebAssembly/wasi-sdk/releases> and set `WASI_SDK_PREFIX` to the extracted directory.

4. Install `icu4c`:

```bash
git clone https://github.com/unicode-org/icu
cd icu/icu4c/source
./configure
make
export ICU_LIBRARY_DIR="$PWD/lib"
export DYLD_LIBRARY_PATH="$DYLD_LIBRARY_PATH:$ICU_LIBRARY_DIR"
```

